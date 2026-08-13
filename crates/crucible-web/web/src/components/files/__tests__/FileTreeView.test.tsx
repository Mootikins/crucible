import { describe, it, expect, vi } from 'vitest';
import { createSignal } from 'solid-js';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { FileTreeView } from '../FileTreeView';
import { makeFileCollection } from '@/lib/file-tree/collection';
import { notesToTree } from '@/lib/file-tree/kiln-builder';
import { itemsForNode, CONTEXT_ITEMS } from '../FileTreeContextMenu';
import type { FileTreeNode } from '@/lib/file-tree/types';

const KILN = '/vault';
const kilnCollection = () =>
  makeFileCollection(
    notesToTree(
      [
        { name: 'Systems.md', path: 'Meta/Systems.md', title: null, tags: [], updated_at: '' },
        { name: 'Roadmap.md', path: 'Meta/Roadmap.md', title: null, tags: [], updated_at: '' },
        { name: 'README.md', path: 'README.md', title: null, tags: [], updated_at: '' },
      ],
      KILN,
    ),
  );

const renderTree = (
  openFilePath: string | null = null,
  onOpenLeaf = vi.fn(),
  onContextAction = vi.fn(),
) => {
  const utils = render(() => (
    <FileTreeView
      collection={kilnCollection()}
      rootKind="kiln"
      openFilePath={openFilePath}
      onOpenLeaf={onOpenLeaf}
      onContextAction={onContextAction}
    />
  ));
  return { ...utils, onOpenLeaf, onContextAction };
};

/** Rows (leaf items and folder headers) currently in the DOM. */
const rows = (container: HTMLElement) => [
  ...container.querySelectorAll<HTMLElement>(
    '[data-scope="tree-view"][data-part="item"], [data-scope="tree-view"][data-part="branch-control"]',
  ),
];

const rowNamed = (container: HTMLElement, name: string) =>
  rows(container).find((r) => r.textContent?.trim() === name)!;

const menuItems = () => [
  ...document.querySelectorAll<HTMLElement>('[data-scope="menu"][data-part="item"]'),
];
const menuItemLabels = () => menuItems().map((i) => i.textContent?.trim());

const openMenuContent = () =>
  document.querySelector<HTMLElement>('[data-scope="menu"][data-part="content"][data-state="open"]');

/** zag highlights on pointerdown and selects the HIGHLIGHTED item on click. */
const chooseMenuItem = (label: string) => {
  const item = menuItems().find((i) => i.textContent?.trim() === label)!;
  fireEvent.pointerDown(item);
  fireEvent.click(item);
};

describe('FileTreeView — rendering & a11y', () => {
  it('renders a role=tree with treeitems carrying aria-level', async () => {
    const { container } = renderTree();
    await waitFor(() => expect(container.querySelector('[role="tree"]')).toBeTruthy());
    const items = container.querySelectorAll('[role="treeitem"]');
    expect(items.length).toBeGreaterThan(0);
    // Top-level items live at aria-level 1.
    expect(container.querySelector('[role="treeitem"][aria-level="1"]')).toBeTruthy();
  });

  it('renders top-level nodes folders-first (Meta dir before README leaf)', async () => {
    const { container, findByText } = renderTree();
    await findByText('Meta');
    const texts = [...container.querySelectorAll('[role="treeitem"]')].map((n) =>
      n.textContent?.trim(),
    );
    // Exact match: `Meta` is collapsed, and a collapsed branch no longer
    // contains its children (lazyMount), so its row text is just its label.
    const metaIdx = texts.findIndex((t) => t === 'Meta');
    const readmeIdx = texts.findIndex((t) => t === 'README.md');
    expect(metaIdx).toBeGreaterThanOrEqual(0);
    expect(readmeIdx).toBeGreaterThan(metaIdx);
  });

  it('opening a leaf routes through selection exactly once (no duplicate side effect)', async () => {
    const onOpenLeaf = vi.fn();
    const { findByText } = renderTree(null, onOpenLeaf);
    const readme = await findByText('README.md');
    fireEvent.click(readme);
    await waitFor(() => expect(onOpenLeaf).toHaveBeenCalledTimes(1));
    expect(onOpenLeaf.mock.calls[0][0]).toMatchObject({ relPath: 'README.md', isDir: false });
  });

  it('clicking a branch does not open a file', async () => {
    const onOpenLeaf = vi.fn();
    const { findByText } = renderTree(null, onOpenLeaf);
    const meta = await findByText('Meta');
    fireEvent.click(meta);
    // Give the machine a tick; a branch selection must never open a leaf.
    await new Promise((r) => setTimeout(r, 0));
    expect(onOpenLeaf).not.toHaveBeenCalled();
  });

  it('marks the open note with aria-current="page"', async () => {
    const { container, findByText } = renderTree('/vault/README.md');
    await findByText('README.md');
    await waitFor(() => {
      const current = container.querySelector('[aria-current="page"]');
      expect(current?.textContent).toContain('README.md');
    });
  });
});

describe('FileTreeView — collapsed subtrees leave the DOM', () => {
  it('never mounts an unexpanded branch’s descendants', async () => {
    const { findByText, queryByText } = renderTree();
    await findByText('Meta');
    // `Meta` starts collapsed: with lazyMount its rows have never rendered, so
    // row cost is paid on first expand, not on tree build.
    expect(queryByText('Systems.md')).toBeNull();
    expect(queryByText('Roadmap.md')).toBeNull();
  });

  it('removes a branch’s descendants from the DOM when it collapses', async () => {
    const { findByText, queryByText } = renderTree();
    const meta = await findByText('Meta');

    fireEvent.click(meta);
    await findByText('Systems.md');

    fireEvent.click(meta);
    // unmountOnExit: collapsing must free the rows, otherwise a session-long
    // browse leaves every branch ever opened mounted forever.
    await waitFor(() => expect(queryByText('Systems.md')).toBeNull());
    expect(queryByText('Roadmap.md')).toBeNull();
  });
});

const PROJECT = '/proj';
/** Project-shaped root: `src` has `children: undefined`, i.e. NOT loaded. */
const lazyProjectRoot = (): FileTreeNode => ({
  relPath: '',
  name: '',
  isDir: true,
  absPath: PROJECT,
  children: [{ relPath: 'src', name: 'src', isDir: true, absPath: `${PROJECT}/src` }],
});

describe('FileTreeView — lazy loading survives unmounting', () => {
  it('re-expands a loaded branch without fetching its children again', async () => {
    const [root, setRoot] = createSignal<FileTreeNode>(lazyProjectRoot());
    const loadChildren = vi.fn(async () => [
      { relPath: 'src/main.rs', name: 'main.rs', isDir: false, absPath: `${PROJECT}/src/main.rs` },
    ]);
    const { findByText, queryByText } = render(() => (
      <FileTreeView
        collection={makeFileCollection(root())}
        rootKind="project"
        openFilePath={null}
        loadChildren={loadChildren}
        onLoadedTree={setRoot}
        onOpenLeaf={() => {}}
        onContextAction={() => {}}
      />
    ));
    // Re-query before every click: persisting the loaded children swaps the
    // collection, which recreates the branch row (a held reference goes stale
    // and clicks on it are silently dropped).
    const clickSrc = async () => fireEvent.click(await findByText('src'));

    await clickSrc();
    await findByText('main.rs');
    expect(loadChildren).toHaveBeenCalledTimes(1);

    await clickSrc();
    await waitFor(() => expect(queryByText('main.rs')).toBeNull());

    await clickSrc();
    await findByText('main.rs');
    // zag's expand path short-circuits on loaded children, so the DOM going
    // away does not re-trigger the fetch.
    expect(loadChildren).toHaveBeenCalledTimes(1);
  });
});

describe('FileTreeView — one context menu for the whole tree', () => {
  it('renders a single context trigger regardless of row count', async () => {
    const { container } = renderTree();
    await waitFor(() => expect(rows(container).length).toBeGreaterThan(1));
    // One trigger + one portalled menu container for N rows — a trigger per row
    // means N zag menu machines and N portalled divs.
    expect(document.querySelectorAll('[data-scope="menu"][data-part="context-trigger"]').length).toBe(1);
    expect(document.querySelectorAll('[data-scope="menu"][data-part="positioner"]').length).toBe(1);
  });

  it('right-clicking a row opens the menu with THAT row\'s actions', async () => {
    const { container, findByText } = renderTree();
    await findByText('README.md');

    fireEvent.contextMenu(rowNamed(container, 'README.md'));
    await waitFor(() => expect(menuItemLabels()).toContain('Copy path'));
    expect(openMenuContent()).toBeTruthy();
    // File row: Open/Rename, never the dir-only creation actions.
    expect(menuItemLabels()).toContain('Open');
    expect(menuItemLabels()).toContain('Rename');
    expect(menuItemLabels()).not.toContain('New folder');

    // The SAME menu retargets when another row is right-clicked: dir actions
    // now, and no Open. (A stale target here is the bug hoisting can introduce.)
    fireEvent.contextMenu(rowNamed(container, 'Meta'));
    await waitFor(() => expect(menuItemLabels()).toContain('New folder'));
    expect(menuItemLabels()).toContain('New note');
    expect(menuItemLabels()).not.toContain('Open');
  });

  it('routes a selected action to the right-clicked node', async () => {
    const onContextAction = vi.fn();
    const { container, findByText } = renderTree(null, vi.fn(), onContextAction);
    await findByText('README.md');

    fireEvent.contextMenu(rowNamed(container, 'README.md'));
    await waitFor(() => expect(menuItemLabels()).toContain('Copy path'));
    chooseMenuItem('Copy path');

    await waitFor(() => expect(onContextAction).toHaveBeenCalledTimes(1));
    expect(onContextAction.mock.calls[0][0]).toBe('copy-path');
    expect(onContextAction.mock.calls[0][1]).toMatchObject({ relPath: 'README.md', isDir: false });
  });

  it('drives the open menu from the keyboard', async () => {
    const onContextAction = vi.fn();
    const { container, findByText } = renderTree(null, vi.fn(), onContextAction);
    await findByText('README.md');

    // The Menu key (and Shift+F10 in Chrome) reaches the tree as a contextmenu
    // event on the focused row — the same path the hoisted trigger listens on.
    fireEvent.contextMenu(rowNamed(container, 'README.md'));
    await waitFor(() => expect(openMenuContent()).toBeTruthy());
    const content = openMenuContent()!;
    fireEvent.keyDown(content, { key: 'ArrowDown' });
    fireEvent.keyDown(content, { key: 'Enter' });

    await waitFor(() => expect(onContextAction).toHaveBeenCalledTimes(1));
    expect(onContextAction.mock.calls[0][0]).toBe('open'); // first item for a file row
    expect(onContextAction.mock.calls[0][1]).toMatchObject({ relPath: 'README.md' });
  });

  it('leaves right-click on empty space below the rows to the browser', async () => {
    const { container, findByText } = renderTree();
    await findByText('README.md');
    const tree = container.querySelector<HTMLElement>('[role="tree"]')!;
    // dispatchEvent returns false only when a listener called preventDefault —
    // the custom menu must not swallow a click that hits no row.
    expect(fireEvent.contextMenu(tree)).toBe(true);
    await new Promise((r) => setTimeout(r, 0));
    expect(menuItemLabels()).toHaveLength(0);
  });

  it('leaves Shift+right-click on a row to the browser', async () => {
    const { container, findByText } = renderTree();
    await findByText('README.md');
    expect(fireEvent.contextMenu(rowNamed(container, 'README.md'), { shiftKey: true })).toBe(true);
    await new Promise((r) => setTimeout(r, 0));
    expect(menuItemLabels()).toHaveLength(0);
  });
});

describe('FileTreeContextMenu action model (itemsForNode)', () => {
  const fileNode: FileTreeNode = { relPath: 'a.md', name: 'a.md', isDir: false, absPath: '/vault/a.md' };
  const dirNode: FileTreeNode = { relPath: 'd', name: 'd', isDir: true, absPath: '/vault/d', children: [] };

  it('offers read actions plus rename/delete on a kiln file', () => {
    const actions = itemsForNode(fileNode, 'kiln').map((i) => i.action);
    expect(actions).toContain('open');
    expect(actions).toContain('reveal-in-tree');
    expect(actions).toContain('copy-path');
    expect(actions).toContain('copy-relative-path');
    expect(actions).toContain('rename');
    expect(actions).toContain('delete');
    // creating INSIDE a file makes no sense — dir-only actions
    expect(actions).not.toContain('new-note');
    expect(actions).not.toContain('new-folder');
  });

  it('gates new-note to kiln folders (projects have no write API for notes)', () => {
    expect(itemsForNode(dirNode, 'kiln').map((i) => i.action)).toContain('new-note');
    expect(itemsForNode(dirNode, 'project').map((i) => i.action)).not.toContain('new-note');
    // ...but folders/rename/delete work on both root kinds (fs.mkdir/fs.trash)
    for (const kind of ['kiln', 'project'] as const) {
      const actions = itemsForNode(dirNode, kind).map((i) => i.action);
      expect(actions).toContain('new-folder');
      expect(actions).toContain('rename');
      expect(actions).toContain('delete');
    }
  });

  it('has no menu entry for move (drag-and-drop owns it)', () => {
    expect(CONTEXT_ITEMS.map((i) => i.action as string)).not.toContain('move');
  });

  it('shows Refresh for a project dir but hides it for a kiln dir (SSE-live)', () => {
    expect(itemsForNode(dirNode, 'project').map((i) => i.action)).toContain('refresh');
    expect(itemsForNode(dirNode, 'kiln').map((i) => i.action)).not.toContain('refresh');
  });

  it('does not offer Open on a directory (files only)', () => {
    expect(itemsForNode(dirNode, 'project').map((i) => i.action)).not.toContain('open');
  });
});
