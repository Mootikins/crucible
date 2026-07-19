import { describe, it, expect, vi } from 'vitest';
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

const renderTree = (openFilePath: string | null = null, onOpenLeaf = vi.fn()) => {
  const utils = render(() => (
    <FileTreeView
      collection={kilnCollection()}
      rootKind="kiln"
      openFilePath={openFilePath}
      onOpenLeaf={onOpenLeaf}
      onContextAction={() => {}}
    />
  ));
  return { ...utils, onOpenLeaf };
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
    const metaIdx = texts.findIndex((t) => t?.startsWith('Meta'));
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

describe('FileTreeContextMenu action model (itemsForNode)', () => {
  const fileNode: FileTreeNode = { relPath: 'a.md', name: 'a.md', isDir: false, absPath: '/vault/a.md' };
  const dirNode: FileTreeNode = { relPath: 'd', name: 'd', isDir: true, absPath: '/vault/d', children: [] };

  it('renders only read-only Phase-1 actions (no Rename/Delete/New)', () => {
    const actions = itemsForNode(fileNode, 'kiln').map((i) => i.action);
    expect(actions).toContain('open');
    expect(actions).toContain('reveal-in-tree');
    expect(actions).toContain('copy-path');
    expect(actions).toContain('copy-relative-path');
    expect(actions).not.toContain('rename');
    expect(actions).not.toContain('delete');
    expect(actions).not.toContain('new-note');
  });

  it('never surfaces any phase-2 item', () => {
    const all = [
      ...itemsForNode(fileNode, 'project'),
      ...itemsForNode(dirNode, 'project'),
      ...itemsForNode(dirNode, 'kiln'),
    ];
    expect(all.every((i) => i.phase === 1)).toBe(true);
    expect(CONTEXT_ITEMS.some((i) => i.phase === 2)).toBe(true); // seam still declared
  });

  it('shows Refresh for a project dir but hides it for a kiln dir (SSE-live)', () => {
    expect(itemsForNode(dirNode, 'project').map((i) => i.action)).toContain('refresh');
    expect(itemsForNode(dirNode, 'kiln').map((i) => i.action)).not.toContain('refresh');
  });

  it('does not offer Open on a directory (files only)', () => {
    expect(itemsForNode(dirNode, 'project').map((i) => i.action)).not.toContain('open');
  });
});
