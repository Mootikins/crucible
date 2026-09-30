/**
 * The Files pane on the mock store and the window store. A port reads
 * `useKilns`, `useProjects` and `useListDir`, and the review counts from
 * `the pending proposals for this path`. The root, the open folders and the order are
 * display state, so they stay in the client.
 */
import { createSignal, type Component } from 'solid-js';
import { openNote, whereFor } from '../actions';
import { KILN_FILES, KILN_PATHS, PROJECT_PATHS, RECENT_FILES } from '../data';
import { FilesPane } from '../components/files/FilesPane';
import type { FileOrder } from '../components/files/FilesNavBar';
import type { RootKey } from '../components/files/FileTree';
import type { RootOptionView } from '../components/files/RootSelector';
import { basename } from '../components/path';
import { buildTree } from '../components/files/tree';
import { TOUCHED, focusedNote, pendingHunks, state } from '../state';
import { setTweak, tweaks } from '../tweaks';
import { openSettings } from './RailContainer';

// The tree holds file names with their extensions. A note path in the store
// has no `.md`, so the container adds it here and strips it on the way out.
const KILN_TREE = buildTree([...KILN_PATHS.map((p) => `${p}.md`), ...KILN_FILES]);
const notePath = (file: string) => file.replace(/\.md$/, '');
const isNote = (file: string) => file.endsWith('.md');

const ROOTS: readonly Omit<RootOptionView, 'attached'>[] = [
  { key: 'docs', label: 'docs', kind: 'kiln' },
  { key: 'crucible', label: 'crucible', kind: 'project' },
  { key: 'folder', label: 'Session folder', kind: 'workspace' },
];

const byName = (a: string, b: string) => basename(a).localeCompare(basename(b));
/** Newest first; a file with no known time falls after, by name. */
const byModified = (a: string, b: string) => {
  const rank = (f: string) => (RECENT_FILES.includes(f) ? RECENT_FILES.indexOf(f) : RECENT_FILES.length);
  return rank(a) - rank(b) || byName(a, b);
};

/** Every folder above a path: `a/b/c` gives `a` and `a/b`. */
const ancestors = (path: string) => path.split('/').slice(0, -1).map((_, i, parts) => parts.slice(0, i + 1).join('/'));

export const FilesPanelContainer: Component = () => {
  const session = () => state.sessions[state.active]!;
  const attached = (root: RootKey) => session().roots.includes(root);
  const [picked, setPicked] = createSignal<RootKey>('docs');
  // The session folder exists only for a session that has one.
  const root = () => (picked() === 'folder' && !attached('folder') ? 'docs' : picked());
  const roots = (): RootOptionView[] =>
    ROOTS.filter((r) => r.key !== 'folder' || attached('folder')).map((r) => ({ ...r, attached: attached(r.key) }));
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set(['Help', 'Help/Concepts']));
  const toggle = (dir: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (!n.delete(dir)) n.add(dir);
      return n;
    });
  const [order, setOrder] = createSignal<FileOrder>('name');
  const reveal = () => {
    const path = focusedNote();
    if (!path) return;
    setPicked('docs');
    setOpen((s) => new Set([...s, ...ancestors(path)]));
    queueMicrotask(() => document.querySelector('.mk-files [aria-current="page"]')?.scrollIntoView({ block: 'nearest' }));
  };
  return (
    <FilesPane
      roots={roots()}
      current={roots().find((r) => r.key === root())!}
      color={session().color}
      onPick={setPicked}
      onManage={openSettings}
      order={order()}
      onOrder={setOrder}
      labels={tweaks.fileLabels}
      onLabels={(v) => setTweak('fileLabels', v)}
      // The mockup writes no files. The real app creates the note or the
      // folder through `lib/query/fs.ts`, then opens a rename in the row.
      onNewNote={() => {}}
      onNewFolder={() => {}}
      onCollapseAll={() => setOpen(new Set<string>())}
      // The mock tree never changes. The real app invalidates `useListDir`.
      onRefresh={() => {}}
      onReveal={reveal}
      root={root()}
      kiln={KILN_TREE}
      projectEntries={PROJECT_PATHS}
      compareFiles={order() === 'name' ? byName : byModified}
      isOpen={(dir) => open().has(dir)}
      onToggle={toggle}
      // The note that last had focus in the centre. A click in this pane
      // moves the focus here, and the row keeps its mark, as in Obsidian.
      activePath={focusedNote() ? `${focusedNote()}.md` : undefined}
      pendingFor={(f) => pendingHunks(state.active).filter((id) => state.hunks[id]!.path === notePath(f)).length}
      touched={(f) => (TOUCHED[state.active] ?? []).includes(notePath(f))}
      // The mockup has no viewer for a canvas, an image or a data file.
      onOpen={(f, e) => isNote(f) && openNote(notePath(f), { where: whereFor(e) })}
    />
  );
};
