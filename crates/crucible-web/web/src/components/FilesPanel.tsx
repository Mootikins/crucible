import { Component, Show, createSignal, createEffect, createMemo, on, onMount, onCleanup } from 'solid-js';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { openFileInEditor, closeTabsUnder } from '@/lib/file-actions';
import { PanelShell } from './PanelShell';
import {
  fetchDirOnce,
  invalidateDirsUnder,
  useFsMkdir,
  useFsMove,
  useFsTrash,
  useListDir,
  useSaveFileContent,
} from '@/lib/query/fs';
import { renamedRel, isValidName } from '@/lib/file-tree/mutations';
import { useKilns } from '@/lib/query/kilns';
import { useConnectSessionKiln } from '@/lib/query/scope';
import { invalidateNotes, useListNotes } from '@/lib/query/notes';
import { fsEvents } from '@/lib/query/sse';
import { moveTargetRel, type FileDragData } from '@/lib/file-dnd';
import type { FsEntry, FsListing } from '@/lib/types';
import { buildRoster, rootKey, type TreeRoot } from '@/lib/tree-root';
import { resolveSessionRoot, sessionRoots, type SessionRoot } from '@/lib/session-roots';
import { NO_SESSION_PIN_KEY, pinnedRootKey, treeRootActions } from '@/stores/treeRootStore';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { SortSpec } from '@/lib/file-tree/types';
import { makeFileCollection, sortTree } from '@/lib/file-tree/collection';
import { notesToTree } from '@/lib/file-tree/kiln-builder';
import {
  createFsEventBatcher,
  reconcileMount,
  type RootMount,
} from '@/lib/file-tree/reconcile';
import { FileTreeView, cssId, type TreeDensity } from './files/FileTreeView';
import { RootDropdown } from './files/RootDropdown';
import type { ContextAction } from './files/FileTreeContextMenu';
import { currentOpenFilePath, revealLoadedPath, revealLazyPath } from './files/file-tree-a11y';
import type { UseTreeViewReturn } from '@ark-ui/solid';
import { ChevronsDownUp, RefreshCw, ArrowUpDown, Plus, Link2 } from '@/lib/icons';
import { EmptyState } from '@/components/ui/EmptyState';

// ---- localStorage helpers (per-root expanded state, global sort) ----------
const EXPANDED_KEY = (rootId: string) => `crucible.filetree.expanded.${rootId}`;
const SORT_KEY = 'crucible.filetree.sort';
const SHOW_HIDDEN_KEY = 'crucible.filetree.showHidden';
const HIDE_EXTS_KEY = 'crucible.filetree.hideExts';
const HIDDEN_EXTS_KEY = 'crucible.filetree.hiddenExts';
const EXPANDED_CAP = 500;
/** Mirrors the daemon's `MAX_DIR_ENTRIES`; used only for the notice text. */
const MAX_LISTED_ENTRIES = 1000;

/** Strip a trailing extension from a display name when it's in `hidden`
 * (Obsidian hides `.md`). Never touches the real node name. */
function formatDisplayName(name: string, hide: boolean, hidden: string[]): string {
  if (!hide) return name;
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return name;
  return hidden.includes(name.slice(dot).toLowerCase()) ? name.slice(0, dot) : name;
}

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

export const FilesPanel: Component<{
  /** Row metrics for the tree. The phone shell passes `touch`. */
  density?: TreeDensity;
}> = (props) => {
  const { projects } = useProjectSafe();
  const { applySessionScope, currentSession } = useSessionSafe();

  // The shell's one kiln roster. Its last-known list paints the tree's roots
  // on reload, so the panel never opens on an empty Browse menu.
  const kilnsQuery = useKilns();
  const kilns = () => kilnsQuery.data ?? [];
  // Every write the tree makes. Each one invalidates the folders it changed,
  // so the panel that made the change does not wait for the daemon's event —
  // and a root the daemon does not watch never sends one.
  const moveEntry = useFsMove();
  const makeFolder = useFsMkdir();
  const trashEntry = useFsTrash();
  const saveFile = useSaveFileContent();
  // The kiln attach. The mutation folds the echoed scope into every cached
  // copy of the session; the echo also goes to the session context below,
  // which owns the selection signal this panel reads.
  const connectKiln = useConnectSessionKiln();
  const [rawRoot, setRawRoot] = createSignal<Node | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [building, setBuilding] = createSignal(false);
  // The panel is busy while the daemon is answering for the root AND while
  // the expanded folders under it are read. The first half belongs to the
  // query now — the note index for a kiln, the top level for a project — so
  // reading only the second would drop the spinner for the whole of the first
  // fetch, the longest part of opening a big root.
  const loading = () => building() || topLevel.isFetching || kilnNotes.isFetching;
  const [sort, setSort] = createSignal<SortSpec>(readJson<SortSpec>(SORT_KEY, DEFAULT_SORT));
  const [showHidden, setShowHidden] = createSignal<boolean>(
    readJson<boolean>(SHOW_HIDDEN_KEY, false),
  );
  const [hideExts, setHideExts] = createSignal<boolean>(readJson<boolean>(HIDE_EXTS_KEY, true));
  const [hiddenExts, setHiddenExts] = createSignal<string[]>(
    readJson<string[]>(HIDDEN_EXTS_KEY, ['.md']),
  );
  const formatName = (name: string) => formatDisplayName(name, hideExts(), hiddenExts());

  const toggleHideExts = () => {
    const next = !hideExts();
    setHideExts(next);
    writeJson(HIDE_EXTS_KEY, next);
  };
  /** Edit the hidden-extension list (comma/space separated; leading dots optional). */
  const editHiddenExts = () => {
    const raw = window.prompt('Hide these extensions (comma-separated):', hiddenExts().join(', '));
    if (raw === null) return;
    const list = raw
      .split(/[\s,]+/)
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean)
      .map((s) => (s.startsWith('.') ? s : '.' + s));
    setHiddenExts(list);
    writeJson(HIDDEN_EXTS_KEY, list);
    if (!hideExts() && list.length) toggleHideExts();
  };

  const toggleHidden = () => {
    const next = !showHidden();
    setShowHidden(next);
    writeJson(SHOW_HIDDEN_KEY, next);
    const r = activeRoot();
    // The listing is keyed by the folder ALONE, because that is all the
    // daemon's stream says, so the same key must answer differently now. The
    // invalidation waits a turn: the hook has to take the new toggle before
    // the refetch reads it, or the refetch asks the old question again.
    if (r?.kind === 'project') queueMicrotask(() => void refreshProjectTree(r));
  };

  // Live machine api (set by FileTreeView.apiRef); powers toolbar actions.
  let treeApi: UseTreeViewReturn<Node> | null = null;

  // Every registered project, worktree and kiln, plus the branches and clone
  // action the dropdown adds. The session's own roots lead the same list.
  const roster = createMemo(() => buildRoster(projects(), kilns()));

  // Roots the daemon refused to list ("not a registered project"). Keyed by
  // path; cleared never — a workspace that 422s once will 422 again this
  // session, and re-offering it just re-traps the tree.
  //
  // Declared ABOVE the memo that reads it. `createMemo` runs its computation
  // eagerly, so a `const` declared further down left the memo reading a name
  // in its temporal dead zone — the panel threw on mount.
  const unlistable = new Set<string>();

  // What this session can browse: its workspace and attached kilns first,
  // then every other registered kiln. Roots the daemon already refused to
  // list are excluded — offering a root that only errors re-traps the tree.
  const roots = createMemo(() => {
    const all = sessionRoots(currentSession(), kilns(), projects());
    const keep = (r: SessionRoot) => !unlistable.has(r.path);
    return {
      own: all.own.filter(keep),
      others: all.others.filter(keep),
    };
  });

  /**
   * The root on screen. Follows the active session unless that session has a
   * pin, so switching sessions re-roots the tree — the whole point of pulling
   * the session list out of this panel.
   */
  const activeRoot = createMemo<SessionRoot | null>(() =>
    resolveSessionRoot(roots(), pinnedRootKey(currentSession()?.id ?? NO_SESSION_PIN_KEY)),
  );

  /**
   * The dropdown's leading section: the session's workspace and its attached
   * kilns. They are the roots the agent can actually read, so they sort above
   * the rest of the registry rather than into it.
   */
  const ownRoots = createMemo<SessionRoot[]>(() => roots().own);

  /**
   * Picking a root PINS it for this session. There is no separate "follow"
   * control: a session with no pin follows, and pinning the root it would
   * have followed to anyway is a no-op in effect. A pin that stops resolving
   * (a renamed kiln, a detached workspace) falls back in
   * `resolveSessionRoot` rather than stranding the tree.
   */
  const selectRoot = (r: TreeRoot) => {
    // No session yet: pin under the session-less key so a cold-start pick
    // still browses. A sessionless browse dies the moment a session appears
    // (its pin map is per-session), which is the right lifetime.
    const id = currentSession()?.id ?? NO_SESSION_PIN_KEY;
    treeRootActions.pin(id, r);
  };

  /**
   * Attach the browsed kiln, so the agent can query what you are reading.
   *
   * Deliberately NOT wired to selection: picking a kiln is navigation, and a
   * navigation gesture must never widen the agent's corpus. The echoed scope
   * folds straight into the session store, so the tab un-dims without a
   * refetch.
   */
  const attachRoot = (r: SessionRoot) => {
    const id = currentSession()?.id;
    if (!id || r.kind !== 'kiln') return;
    void (async () => {
      try {
        applySessionScope(await connectKiln.mutateAsync({ id, kiln: r.name }));
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : `Failed to attach ${r.name}`);
      }
    })();
  };

  // ---- data-source discriminant --------------------------------------------

  /**
   * The browsed kiln, or nothing while a project is on screen.
   *
   * A kiln's whole tree comes from its note index in ONE answer, which is why
   * a kiln has no per-folder read and a project does.
   */
  const kilnRootPath = createMemo<string | null>(() => {
    const root = activeRoot();
    return root && root.kind === 'kiln' ? root.path : null;
  });

  /**
   * That index, as a cache entry.
   *
   * The command palette and the canvas note picker hold the same entry, so a
   * palette opened over this panel asks the daemon nothing. The refresh action
   * invalidates it (`reloadRoot`) rather than calling a loader, which is the
   * same shape the project side already has.
   */
  const kilnNotes = useListNotes(kilnRootPath);

  /**
   * A root the daemon refused to list.
   *
   * A workspace that is not a registered project — `~/.crucible`, a scratch
   * tmpdir a session was born in — must not strand the tree on an error
   * banner: remember it as unlistable so the strip stops offering it and the
   * active root falls through to something browsable.
   */
  function failRoot(root: TreeRoot, e: unknown) {
    setRawRoot(null);
    unlistable.add(root.path);
    setError(e instanceof Error && e.message ? e.message : `Failed to list ${root.path}`);
  }

  /**
   * The browsed project root, or nothing while a kiln is on screen.
   *
   * A kiln's tree comes from `listNotes` in one answer, so only a project has
   * a top level to hold.
   */
  const projectRoot = createMemo<TreeRoot | null>(() => {
    const root = activeRoot();
    return root && root.kind === 'project' ? root : null;
  });

  /**
   * The top level of the browsed project, as a cache entry.
   *
   * It is held under the ABSOLUTE folder, which is the name the daemon's
   * filesystem stream uses, so a write anywhere in that folder reaches this
   * panel — and reaches a second panel browsing the same root without a
   * second fetch. `showHidden` is not in the key, so the toggle invalidates
   * (`toggleHidden`) rather than keying a second entry the stream cannot name.
   */
  const topLevel = useListDir(() => {
    const root = projectRoot();
    return root ? { root: root.path, relPath: '', showHidden: showHidden() } : null;
  });

  // Build the tree from that top level, eagerly re-reading every
  // persisted-expanded folder so the tree rehydrates its open branches on
  // reload. A flat top-level build would discard the loaded subtrees — the
  // machine then paints the persisted-expanded nodes as empty, which reads as
  // the tree spontaneously collapsing (the bug this replaces).
  async function buildProjectTree(root: TreeRoot, listing: FsListing) {
    setBuilding(true);
    setError(null);
    const expanded = new Set(expandedFor(root));
    let anyTruncated = listing.truncated;
    const build = async (entries: FsEntry[]): Promise<Node[]> =>
      Promise.all(
        entries.map(async (e) => {
          const node = fsEntryToNode(e, root.path);
          if (node.isDir && expanded.has(node.relPath)) {
            const child = await fetchDirOnce({
              root: root.path,
              relPath: node.relPath,
              showHidden: showHidden(),
            });
            anyTruncated ||= child.truncated;
            node.children = await build(child.entries);
          }
          return node;
        }),
      );
    try {
      const children = await build(listing.entries);
      setRawRoot({ relPath: '', name: '', isDir: true, absPath: root.path, children });
      // Say so rather than presenting a capped folder as complete. Non-fatal,
      // so it rides the same banner as the move/link notices.
      if (anyTruncated) {
        setError(`Some folders have more than ${MAX_LISTED_ENTRIES} entries; showing the first ${MAX_LISTED_ENTRIES}`);
      }
    } catch (e) {
      failRoot(root, e);
    } finally {
      setBuilding(false);
    }
  }

  /**
   * Re-reads a project root from the daemon.
   *
   * The listings are cache entries now, so a refresh is an invalidation: the
   * top level is observed and re-reads itself, and the rebuild below re-reads
   * the expanded folders under it.
   */
  const refreshProjectTree = (root: TreeRoot) => invalidateDirsUnder(root.path);

  // Keyed on the root's identity AS A PATH, not on the memo's object. `roster()`
  // rebuilds fresh TreeRoot objects on every recompute and the kiln query
  // answers twice by design (stored list, then fetched), so the roster changes
  // twice per mount — and an identity-keyed effect refetched the root plus every
  // persisted-expanded folder a second time. That was the duplicate
  // `/api/fs/list` per expand: folders already in the persisted-expanded set
  // were fetched once per pass.
  //
  // `on`'s handler also runs untracked, which drops a second accidental
  // dependency: the build reads `showHidden()` before its first await (so
  // inside the tracking scope), and `toggleHidden` already reloads explicitly
  // — tracking it meant every toggle did two full loads.
  // The key is its OWN memo, and that is the load-bearing part: `on()` narrows
  // what an effect tracks but does not compare the dep's value, so keying it on
  // an inline accessor still re-ran on every `activeRoot` notification. A memo
  // compares with `===`, so an unchanged key string stops the propagation here.
  const activeRootKey = createMemo(() => {
    const root = activeRoot();
    return root ? rootKey(root) : null;
  });

  createEffect(
    on(activeRootKey, (key) => {
      setRawRoot(null);
      if (!key) return;
      // Neither kind starts a read here. A project root builds from
      // `topLevel` and a kiln from `kilnNotes`, both of which are already
      // asking for it: a second read here is the duplicate fetch this panel
      // spent three fixes removing.
    }),
  );

  // The kiln tree, rebuilt from the index whenever the cache answers — which
  // is on the first read, on a refresh, and on an invalidation from anywhere
  // else holding the same entry.
  createEffect(
    on(
      () => [kilnNotes.data, kilnNotes.dataUpdatedAt] as const,
      ([notes]) => {
        const kiln = kilnRootPath();
        if (!kiln || !notes) return;
        setRawRoot(notesToTree(notes, kiln));
        setError(null);
      },
    ),
  );

  createEffect(() => {
    const failure = kilnNotes.error;
    if (!kilnRootPath() || !failure) return;
    setRawRoot(null);
    setError(failure.message || 'Failed to load notes');
  });

  // `dataUpdatedAt` moves on every answer, including one that equals the last.
  // Without it a refresh that found no change would leave the expanded folders
  // unread, so a change DEEPER than the top level would never reach the tree.
  createEffect(
    on(
      () => [topLevel.data, topLevel.dataUpdatedAt] as const,
      ([listing]) => {
        const root = projectRoot();
        if (!root || !listing) return;
        void buildProjectTree(root, listing);
      },
    ),
  );

  createEffect(() => {
    const failure = topLevel.error;
    const root = projectRoot();
    if (failure && root) failRoot(root, failure);
  });

  // Displayed collection = sorted view of the raw tree. A new identity on
  // raw-tree or sort change reaches the tree machine reactively — it does NOT
  // remount FileTreeView any more; see the note at the <Show> below.
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
    const { entries, truncated } = await fetchDirOnce({
      root: root.path,
      relPath: details.node.relPath,
      showHidden: showHidden(),
    });
    setError(
      truncated
        ? `${details.node.name} has more than ${MAX_LISTED_ENTRIES} entries; showing the first ${MAX_LISTED_ENTRIES}`
        : null,
    );
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
        const outcome = await moveEntry.mutateAsync({
          root: root.path,
          kind: root.kind,
          fromRel: source.relPath,
          toRel,
        });
        // Kiln .md moves rewrite inbound wikilinks daemon-side; ambiguous
        // ones are deliberately skipped — tell the user instead of silently
        // leaving links pointing elsewhere.
        const skipped = outcome.skipped?.length ?? 0;
        setError(
          skipped > 0
            ? `Moved, but ${skipped} link${skipped === 1 ? '' : 's'} not auto-updated (ambiguous target)`
            : null,
        );
        await reloadRoot(root);
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
    if (root.kind === 'kiln') await invalidateNotes(root.path);
    else await refreshProjectTree(root);
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
        const outcome = await moveEntry.mutateAsync({
          root: root.path,
          kind: root.kind,
          fromRel: relPath,
          toRel,
        });
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
      await saveFile.mutateAsync({ path: abs, content: '' });
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
        if (root.kind === 'project') void refreshProjectTree(root);
        break;
      case 'toggle-hidden':
        toggleHidden();
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
          await makeFolder.mutateAsync({ root: root.path, kind: root.kind, relPath: rel });
          treeApi?.().expand([node.relPath]);
        });
        break;
      }
      case 'delete': {
        if (!window.confirm(`Move "${node.name}" to trash?`)) break;
        runMutation(root, async () => {
          await trashEntry.mutateAsync({
            root: root.path,
            kind: root.kind,
            relPath: node.relPath,
          });
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
    scrollRelIntoView(rel);
  }

  /** Scroll a revealed row into view. Deferred so lazy-expanded ancestors have
   * mounted their children before we look the node up. `block: 'nearest'`
   * avoids yanking the viewport when the row is already visible. */
  function scrollRelIntoView(rel: string) {
    window.setTimeout(() => {
      document
        .getElementById(`filetree-node-${cssId(rel)}`)
        ?.scrollIntoView({ block: 'nearest' });
    }, 60);
  }

  // No auto-reveal. Following the focused tab moved the navigator out from
  // under the pointer — it expanded a branch and scrolled on every tab switch,
  // which fights browsing: the tree is where you go to look somewhere ELSE than
  // the file you are editing. `revealActive` stays for the explicit
  // `reveal-in-tree` action, which is where VSCode's behaviour actually belongs.

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
      void refreshProjectTree(root);
    }
  });

  /**
   * Open the Browse root dropdown from the empty state.
   *
   * The dropdown owns its open flag, so the empty state clicks its trigger
   * instead of holding a second copy of that state. The click is scoped to
   * the header row this panel renders, so it can never reach another panel's
   * dropdown. `RootDropdown` always renders its trigger, including with an
   * empty roster — that is the Clone door — so this never misses.
   */
  let rootBarRef: HTMLDivElement | undefined;
  const openRootDropdown = () => {
    rootBarRef?.querySelector<HTMLButtonElement>('[data-testid="root-dropdown"]')?.click();
  };

  onMount(() => {
    const onToggleHidden = () => toggleHidden();
    window.addEventListener('crucible:toggle-hidden-files', onToggleHidden);
    onCleanup(() => window.removeEventListener('crucible:toggle-hidden-files', onToggleHidden));
    // One source for the stream, whatever the count of readers: `fsEvents()`
    // is the shared root of `lib/query/sse.ts`, and the editor watches the same
    // stream for its open buffers. Each side keeps its own handler on it.
    const unsub = fsEvents().subscribe((ev) => batcher.push(ev));
    // Project roots are refresh-on-interaction: refetch expanded folders on focus.
    const onFocus = () => {
      const root = activeRoot();
      if (root?.kind === 'project') void refreshProjectTree(root);
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
      {/* No "Files" heading — the panel tab already names it. The dropdown
          leads so the browsed root reads as the panel's title. */}
      <div class="shrink-0 flex items-center justify-between gap-2 p-3 border-b border-hairline">
        <div class="flex items-center gap-1 min-w-0 flex-1" ref={rootBarRef}>
          <RootDropdown
            own={ownRoots()}
            groups={roster()}
            selectedKey={activeRoot() ? rootKey(activeRoot()!) : null}
            onSelect={selectRoot}
            activeRoot={activeRoot()}
            onNotice={setError}
          />

          {/* Only on the ACTIVE unattached kiln: an affordance for a root you
              are not looking at would be a claim about a corpus you cannot
              see. */}
          <Show when={activeRoot()?.origin === 'other-kiln' ? activeRoot() : null} keyed>
            {(root) => (
              <button
                type="button"
                data-testid="root-attach"
                title={`Let this session query ${root.name}`}
                onClick={() => attachRoot(root)}
                class="flex items-center gap-1 px-1.5 py-1 rounded text-floor text-muted hover:text-shell-ink hover:bg-hover-wash whitespace-nowrap transition-colors"
              >
                <Link2 class="w-3 h-3" /> Attach
              </button>
            )}
          </Show>
        </div>
        <div class="flex items-center gap-1 shrink-0">
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
          <button
            type="button"
            aria-label="Hide extensions"
            title={`Hide extensions (${hiddenExts().join(' ') || 'none'}) — right-click to edit`}
            aria-pressed={hideExts()}
            onClick={toggleHideExts}
            onContextMenu={(e) => { e.preventDefault(); editHiddenExts(); }}
            classList={{ 'p-1 rounded hover:bg-hover-wash text-floor font-mono leading-none w-6 h-6 flex items-center justify-center': true, 'text-primary': hideExts(), 'text-muted': !hideExts() }}
          >
            .ext
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
                if (r) void refreshProjectTree(r);
              }}
              class="p-1 rounded hover:bg-hover-wash text-muted"
            >
              <RefreshCw class="w-3.5 h-3.5" />
            </button>
          </Show>
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
          <EmptyState
            title="No project or kiln to browse"
            body="Pick a root, or clone a repository from the same menu."
            action={{ label: 'Choose a project', onClick: openRootDropdown }}
            testid="files-empty"
          />
        </Show>
        {/* NOT `keyed`. Keyed, every lazily loaded folder rebuilt the entire
            tree: `onLoadedTree` -> `setRawRoot` -> new `collection` identity ->
            this Show tears down and recreates every row. Measured at 1134
            visible rows, expanding a folder with TWO children cost 1775ms of
            DOM work against 4ms of network, and mutation totals showed
            `removed == rowsBefore` each time — the cost tracked total rows
            (~1.3ms each), not children added.

            Redundant as well as expensive: `FileTreeView` declares its machine
            options as a function of props (`useTreeView(() => ({ collection:
            props.collection, ... }))`), so a new collection already reaches the
            machine reactively. Persisting the merged tree — the thing the
            discarded-children bug was about — is `onLoadedTree` -> `setRawRoot`
            below, and that is untouched. */}
        <Show when={collection()}>
          {(col) => {
            const root = activeRoot()!;
            return (
              <FileTreeView
                collection={col()}
                rootKind={root.kind}
                density={props.density}
                openFilePath={openFilePath()}
                defaultExpandedValue={expandedFor(root)}
                loadChildren={root.kind === 'project' ? loadChildren(root) : undefined}
                onLoadedTree={root.kind === 'project' ? (rootNode) => setRawRoot(rootNode) : undefined}
                onOpenLeaf={onOpenLeaf}
                onExpandedChange={(values) => persistExpanded(root, values)}
                onContextAction={onContextAction}
                apiRef={(api) => (treeApi = api)}
                showHidden={showHidden()}
                formatName={formatName}
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
