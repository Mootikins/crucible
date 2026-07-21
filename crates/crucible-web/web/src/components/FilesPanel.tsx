import { Component, Show, createSignal, createEffect, createMemo, onMount, onCleanup } from 'solid-js';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { openFileInEditor, closeTabsUnder } from '@/lib/file-actions';
import { PanelShell } from './PanelShell';
import {
  listNotes,
  listDir,
  listKilns,
  subscribeToFsEvents,
  fsMove,
  fsMkdir,
  fsTrash,
  saveFileContent,
} from '@/lib/api';
import { renamedRel, isValidName } from '@/lib/file-tree/mutations';
import { moveTargetRel, type FileDragData } from '@/lib/file-dnd';
import type { KilnListEntry, FsEntry } from '@/lib/types';
import { buildRoster, rosterIndex, rootKey, type TreeRoot } from '@/lib/tree-root';
import { selectedRootKey, treeRootActions } from '@/stores/treeRootStore';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { SortSpec } from '@/lib/file-tree/types';
import { makeFileCollection, sortTree } from '@/lib/file-tree/collection';
import { notesToTree } from '@/lib/file-tree/kiln-builder';
import {
  createFsEventBatcher,
  reconcileMount,
  type RootMount,
} from '@/lib/file-tree/reconcile';
import { FileTreeView } from './files/FileTreeView';
import { RootDropdown } from './files/RootDropdown';
import type { ContextAction } from './files/FileTreeContextMenu';
import { currentOpenFilePath, revealLoadedPath, revealLazyPath } from './files/file-tree-a11y';
import type { UseTreeViewReturn } from '@ark-ui/solid';
import { ChevronsDownUp, RefreshCw, ArrowUpDown, Plus } from '@/lib/icons';

// ---- localStorage helpers (per-root expanded state, global sort) ----------
const EXPANDED_KEY = (rootId: string) => `crucible.filetree.expanded.${rootId}`;
const SORT_KEY = 'crucible.filetree.sort';
const EXPANDED_CAP = 500;

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* private mode */
  }
}

const DEFAULT_SORT: SortSpec = { key: 'name', dir: 'asc' };

/** FsEntry (wire) -> FileTreeNode. Dirs get `children: undefined` (lazy). */
function fsEntryToNode(e: FsEntry, rootPath: string): Node {
  return {
    relPath: e.rel_path,
    name: e.name,
    isDir: e.is_dir,
    absPath: `${rootPath}/${e.rel_path}`,
    modified: e.modified ?? undefined,
  };
}

export const FilesPanel: Component = () => {
  const { projects } = useProjectSafe();

  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [rawRoot, setRawRoot] = createSignal<Node | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [sort, setSort] = createSignal<SortSpec>(readJson<SortSpec>(SORT_KEY, DEFAULT_SORT));

  // Live machine api (set by FileTreeView.apiRef); powers toolbar actions.
  let treeApi: UseTreeViewReturn<Node> | null = null;

  onMount(async () => {
    try {
      setKilns(await listKilns());
    } catch (e) {
      console.error('Failed to list kilns:', e);
    }
  });

  const roster = createMemo(() => buildRoster(projects(), kilns()));

  const activeRoot = createMemo<TreeRoot | null>(() => {
    const groups = roster();
    const idx = rosterIndex(groups);
    const persisted = selectedRootKey();
    if (persisted && idx.has(persisted)) return idx.get(persisted)!;
    const firstProject = groups.find((g) => g.kind === 'project')?.roots[0];
    const firstKiln = groups.find((g) => g.kind === 'kiln')?.roots[0];
    return firstProject ?? firstKiln ?? null; // deterministic fallback
  });

  // Keep the persisted key in sync with the resolved fallback (silent, no persist).
  createEffect(() => {
    const r = activeRoot();
    if (r) treeRootActions.setSelectedRootKey(selectedRootKey() ?? rootKey(r));
  });

  // ---- data-source discriminant --------------------------------------------
  async function loadKilnTree(kilnPath: string) {
    setLoading(true);
    setError(null);
    try {
      const notes = await listNotes(kilnPath);
      setRawRoot(notesToTree(notes, kilnPath));
    } catch (e) {
      setRawRoot(null);
      setError(e instanceof Error ? e.message : 'Failed to load notes');
    } finally {
      setLoading(false);
    }
  }

  async function loadProjectDir(rootPath: string, rel: string) {
    setLoading(true);
    setError(null);
    try {
      const entries = await listDir(rootPath, rel);
      const children = entries.map((e) => fsEntryToNode(e, rootPath));
      setRawRoot({ relPath: '', name: '', isDir: true, absPath: rootPath, children });
    } catch (e) {
      setRawRoot(null);
      setError(e instanceof Error ? e.message : 'Failed to list directory');
    } finally {
      setLoading(false);
    }
  }

  createEffect(() => {
    const root = activeRoot();
    setRawRoot(null);
    if (!root) return;
    if (root.kind === 'kiln') void loadKilnTree(root.path);
    else void loadProjectDir(root.path, '');
  });

  // Displayed collection = sorted view of the raw tree. New identity on
  // raw-tree or sort change -> FileTreeView re-mounts (keyed <Show>).
  const collection = createMemo(() => {
    const raw = rawRoot();
    return raw ? makeFileCollection(sortTree(raw, sort())) : null;
  });

  const openFilePath = createMemo(() => currentOpenFilePath());

  const expandedFor = (r: TreeRoot) => readJson<string[]>(EXPANDED_KEY(rootKey(r)), []);
  const persistExpanded = (r: TreeRoot, values: string[]) =>
    writeJson(EXPANDED_KEY(rootKey(r)), values.slice(0, EXPANDED_CAP));

  // Project lazy loader (kilns build the whole tree so they pass undefined).
  const loadChildren = (root: TreeRoot) => async (details: { node: Node }) => {
    const entries = await listDir(root.path, details.node.relPath);
    return entries.map((e) => fsEntryToNode(e, root.path));
  };

  const onOpenLeaf = (node: Node) => openFileInEditor(node.absPath, node.name);

  // ---- drag-and-drop move --------------------------------------------------
  // Refresh-after-move (not optimistic patching): the tree remounts on
  // collection identity change, persisted expanded state re-expands, and lazy
  // project folders refetch — so a full reload is both simple and correct.
  // Kiln SSE events will also arrive; the reconcile path is idempotent.
  const onDndMove = (source: FileDragData, destParentRel: string) => {
    const root = activeRoot();
    if (!root) return;
    const toRel = moveTargetRel(source, destParentRel);
    void (async () => {
      try {
        const outcome = await fsMove(root.path, root.kind, source.relPath, toRel);
        // Kiln .md moves rewrite inbound wikilinks daemon-side; ambiguous
        // ones are deliberately skipped — tell the user instead of silently
        // leaving links pointing elsewhere.
        const skipped = outcome.skipped?.length ?? 0;
        setError(
          skipped > 0
            ? `Moved, but ${skipped} link${skipped === 1 ? '' : 's'} not auto-updated (ambiguous target)`
            : null,
        );
        if (root.kind === 'kiln') await loadKilnTree(root.path);
        else await loadProjectDir(root.path, '');
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Move failed');
      }
    })();
  };

  const dndFor = (root: TreeRoot) => ({
    rootId: rootKey(root),
    rootKind: root.kind,
    rootPath: root.path,
    onMove: onDndMove,
    expandBranch: (relPath: string) => treeApi?.().expand([relPath]),
  });

  const reloadRoot = async (root: TreeRoot) => {
    if (root.kind === 'kiln') await loadKilnTree(root.path);
    else await loadProjectDir(root.path, '');
  };

  /** Surface a mutation failure in the banner without killing the tree. */
  const runMutation = (root: TreeRoot, op: () => Promise<void>) => {
    void (async () => {
      try {
        await op();
        setError(null);
        await reloadRoot(root);
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Operation failed');
      }
    })();
  };

  /** Inline-rename commit (ark machine → fs.move; kiln .md renames rewrite links daemon-side). */
  const onRenameNode = (relPath: string, newLabel: string) => {
    const root = activeRoot();
    if (!root || !isValidName(newLabel)) return;
    const toRel = renamedRel(relPath, newLabel);
    if (toRel === relPath) return;
    void (async () => {
      try {
        const outcome = await fsMove(root.path, root.kind, relPath, toRel);
        const skipped = outcome.skipped?.length ?? 0;
        setError(
          skipped > 0
            ? `Renamed, but ${skipped} link${skipped === 1 ? '' : 's'} not auto-updated (ambiguous target)`
            : null,
        );
        await reloadRoot(root);
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Rename failed');
      }
    })();
  };

  /** Create a note inside `dirRel` (kiln only — projects have no write API). */
  const newNoteIn = (root: TreeRoot, dirRel: string) => {
    const name = window.prompt('Note name', 'Untitled');
    if (name === null || !isValidName(name)) return;
    const file = name.includes('.') ? name : `${name}.md`;
    const rel = dirRel ? `${dirRel}/${file}` : file;
    const abs = `${root.path}/${rel}`;
    runMutation(root, async () => {
      await saveFileContent(abs, '');
      if (dirRel) treeApi?.().expand([dirRel]);
      openFileInEditor(abs, file);
    });
  };

  const onContextAction = (action: ContextAction, node: Node) => {
    const root = activeRoot();
    if (!root) return;
    switch (action) {
      case 'open':
        openFileInEditor(node.absPath, node.name);
        break;
      case 'reveal-in-tree':
        revealActive(node.relPath);
        break;
      case 'copy-path':
        void navigator.clipboard?.writeText(node.absPath);
        break;
      case 'copy-relative-path':
        void navigator.clipboard?.writeText(node.relPath);
        break;
      case 'refresh':
        // Project-only: refetch this folder (top-level refetch keeps it simple).
        if (root.kind === 'project') void loadProjectDir(root.path, '');
        break;
      case 'rename':
        // Defer past the context menu's close + focus restoration: the menu
        // returns focus to the row AFTER onSelect, which blurs a just-mounted
        // rename input and silently cancels the rename.
        window.setTimeout(() => treeApi?.().startRenaming(node.relPath), 120);
        break;
      case 'new-note':
        newNoteIn(root, node.relPath);
        break;
      case 'new-folder': {
        const name = window.prompt('Folder name', 'New folder');
        if (name === null || !isValidName(name)) break;
        const rel = node.relPath ? `${node.relPath}/${name}` : name;
        runMutation(root, async () => {
          await fsMkdir(root.path, root.kind, rel);
          treeApi?.().expand([node.relPath]);
        });
        break;
      }
      case 'delete': {
        if (!window.confirm(`Move "${node.name}" to trash?`)) break;
        runMutation(root, async () => {
          await fsTrash(root.path, root.kind, node.relPath);
          closeTabsUnder(node.absPath, node.isDir);
        });
        break;
      }
    }
  };

  // ---- toolbar actions -----------------------------------------------------
  const collapseAll = () => treeApi?.().collapse();

  function revealActive(relPathOverride?: string) {
    const root = activeRoot();
    const col = collection();
    if (!root || !treeApi || !col) return;
    let rel = relPathOverride;
    if (!rel) {
      const open = openFilePath();
      if (!open) return;
      const base = root.path.replace(/\/+$/, '');
      if (open !== base && !open.startsWith(base + '/')) return;
      rel = open === base ? '' : open.slice(base.length + 1);
    }
    if (!rel) return;
    const api = treeApi();
    if (root.kind === 'kiln') {
      revealLoadedPath(api, col, rel);
    } else {
      void revealLazyPath(
        {
          expand: (v) => api.expand(v),
          focus: (v) => api.focus(v),
          onLoaded: async () => Promise.resolve(),
        },
        rel,
      );
    }
  }

  const cycleSort = () => {
    const s = sort();
    // name-asc -> name-desc -> modified-desc -> modified-asc -> name-asc
    const order: SortSpec[] = [
      { key: 'name', dir: 'asc' },
      { key: 'name', dir: 'desc' },
      { key: 'modified', dir: 'desc' },
      { key: 'modified', dir: 'asc' },
    ];
    const i = order.findIndex((o) => o.key === s.key && o.dir === s.dir);
    const next = order[(i + 1) % order.length];
    setSort(next);
    writeJson(SORT_KEY, next);
  };

  // ---- live SSE reconcile (kilns patch in-memory; projects refetch) --------
  const batcher = createFsEventBatcher(150, (events) => {
    const root = activeRoot();
    const raw = rawRoot();
    if (!root || !raw) return;
    const mount: RootMount = {
      rootId: rootKey(root),
      kind: root.kind,
      basePath: root.path,
      root: raw,
    };
    const { root: patched, invalidate } = reconcileMount(mount, events);
    if (patched) setRawRoot(patched);
    if (invalidate && invalidate.length > 0 && root.kind === 'project') {
      // Defensive path (unused in P1: only kiln dirs are watched). Any loaded
      // folder change -> refetch the whole top level (keeps it simple).
      void loadProjectDir(root.path, '');
    }
  });

  onMount(() => {
    const unsub = subscribeToFsEvents((ev) => batcher.push(ev));
    // Project roots are refresh-on-interaction: refetch expanded folders on focus.
    const onFocus = () => {
      const root = activeRoot();
      if (root?.kind === 'project') void loadProjectDir(root.path, '');
    };
    window.addEventListener('focus', onFocus);
    onCleanup(() => {
      unsub();
      batcher.dispose();
      window.removeEventListener('focus', onFocus);
    });
  });

  return (
    <PanelShell class="overflow-hidden">
      <div class="p-3 border-b border-hairline shrink-0 flex items-center justify-between gap-2">
        <h2 class="text-sm font-semibold text-muted uppercase tracking-wide">Files</h2>
        <div class="flex items-center gap-1">
          <button
            type="button"
            aria-label="Sort"
            title="Cycle sort (name / modified)"
            onClick={cycleSort}
            class="p-1 rounded hover:bg-hover-wash text-muted"
          >
            <ArrowUpDown class="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            aria-label="Collapse all"
            title="Collapse all"
            onClick={collapseAll}
            class="p-1 rounded hover:bg-hover-wash text-muted"
          >
            <ChevronsDownUp class="w-3.5 h-3.5" />
          </button>
          <Show when={activeRoot()?.kind === 'kiln'}>
            <button
              type="button"
              aria-label="New note"
              title="New note"
              onClick={() => {
                const r = activeRoot();
                if (r) newNoteIn(r, '');
              }}
              class="p-1 rounded hover:bg-hover-wash text-muted"
            >
              <Plus class="w-3.5 h-3.5" />
            </button>
          </Show>
          <Show when={activeRoot()?.kind === 'project'}>
            <button
              type="button"
              aria-label="Refresh"
              title="Refresh"
              onClick={() => {
                const r = activeRoot();
                if (r) void loadProjectDir(r.path, '');
              }}
              class="p-1 rounded hover:bg-hover-wash text-muted"
            >
              <RefreshCw class="w-3.5 h-3.5" />
            </button>
          </Show>
          <RootDropdown
            groups={roster()}
            selectedKey={selectedRootKey()}
            onSelect={(r) => treeRootActions.selectRoot(r)}
          />
        </div>
      </div>

      <div class="flex-1 overflow-y-auto py-2">
        <Show
          when={error()}
        >
          <div class="mx-3 my-2 px-3 py-2 text-sm text-error bg-error/10 rounded border border-error/30">
            {error()}
          </div>
        </Show>
        <Show when={loading() && !rawRoot()}>
          <div class="px-3 py-2 text-muted-dark text-sm">Loading…</div>
        </Show>
        <Show when={!activeRoot()}>
          <div class="px-3 py-8 text-center text-muted-dark text-sm">
            No project or kiln to browse
          </div>
        </Show>
        <Show when={collection()} keyed>
          {(col) => {
            const root = activeRoot()!;
            return (
              <FileTreeView
                collection={col}
                rootKind={root.kind}
                openFilePath={openFilePath()}
                defaultExpandedValue={expandedFor(root)}
                loadChildren={root.kind === 'project' ? loadChildren(root) : undefined}
                onLoadedTree={root.kind === 'project' ? (rootNode) => setRawRoot(rootNode) : undefined}
                onOpenLeaf={onOpenLeaf}
                onExpandedChange={(values) => persistExpanded(root, values)}
                onContextAction={onContextAction}
                apiRef={(api) => (treeApi = api)}
                dnd={dndFor(root)}
                onRenameNode={onRenameNode}
              />
            );
          }}
        </Show>
      </div>
    </PanelShell>
  );
};
