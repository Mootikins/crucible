// src/lib/file-dnd.test.ts
import { describe, it, expect } from 'vitest';
import {
  canDropIntoFolder,
  moveTargetRel,
  insertTextFor,
  isInnermostFileTarget,
  isFileDragData,
  type FileDragData,
} from './file-dnd';

const drag = (over: Partial<FileDragData> = {}): FileDragData => ({
  type: 'fileNode',
  rootId: 'kiln:/k',
  rootKind: 'kiln',
  rootPath: '/k',
  relPath: 'notes/a.md',
  absPath: '/k/notes/a.md',
  name: 'a.md',
  isDir: false,
  ...over,
});

describe('isFileDragData', () => {
  it('accepts fileNode payloads and rejects tab payloads', () => {
    expect(isFileDragData(drag())).toBe(true);
    expect(isFileDragData({ type: 'tab', tabId: 't1' })).toBe(false);
    expect(isFileDragData({})).toBe(false);
  });
});

describe('canDropIntoFolder', () => {
  it('allows a file into a different folder of the same root', () => {
    expect(canDropIntoFolder(drag(), { rootId: 'kiln:/k', relPath: 'archive' })).toBe(true);
  });

  it('rejects cross-root drops (fs.move is within-root)', () => {
    expect(canDropIntoFolder(drag(), { rootId: 'project:/p', relPath: 'archive' })).toBe(false);
  });

  it("rejects dropping into the node's current parent (no-op)", () => {
    expect(canDropIntoFolder(drag(), { rootId: 'kiln:/k', relPath: 'notes' })).toBe(false);
    const topLevel = drag({ relPath: 'a.md' });
    expect(canDropIntoFolder(topLevel, { rootId: 'kiln:/k', relPath: '' })).toBe(false);
  });

  it('rejects a directory dropped into itself or its own descendant', () => {
    const dir = drag({ relPath: 'notes', name: 'notes', isDir: true });
    expect(canDropIntoFolder(dir, { rootId: 'kiln:/k', relPath: 'notes' })).toBe(false);
    expect(canDropIntoFolder(dir, { rootId: 'kiln:/k', relPath: 'notes/sub' })).toBe(false);
    // Sibling with a shared name PREFIX is not a descendant.
    expect(canDropIntoFolder(dir, { rootId: 'kiln:/k', relPath: 'notes-old' })).toBe(true);
  });

  it('allows a directory into an unrelated folder and into the root', () => {
    const dir = drag({ relPath: 'notes/deep', name: 'deep', isDir: true });
    expect(canDropIntoFolder(dir, { rootId: 'kiln:/k', relPath: 'archive' })).toBe(true);
    expect(canDropIntoFolder(dir, { rootId: 'kiln:/k', relPath: '' })).toBe(true);
  });
});

describe('moveTargetRel', () => {
  it('joins the destination folder with the source name', () => {
    expect(moveTargetRel(drag(), 'archive')).toBe('archive/a.md');
  });
  it('moves to the root with a bare name', () => {
    expect(moveTargetRel(drag(), '')).toBe('a.md');
  });
});

describe('insertTextFor', () => {
  it('inserts a wikilink for a kiln markdown note dropped into markdown', () => {
    expect(insertTextFor(drag(), 'journal/today.md')).toBe('[[a]]');
  });

  it('inserts the rel path for a project file dropped into markdown', () => {
    const src = drag({ rootKind: 'project', rootId: 'project:/p', relPath: 'src/main.rs', name: 'main.rs' });
    expect(insertTextFor(src, 'README.md')).toBe('src/main.rs');
  });

  it('inserts the rel path when the target is not markdown', () => {
    expect(insertTextFor(drag(), 'src/main.rs')).toBe('notes/a.md');
  });
});

describe('isInnermostFileTarget', () => {
  const el = (id: string) => ({ id }) as unknown as Element;

  it('is true only for the first zone-tagged target in the stack', () => {
    const editor = el('editor');
    const pane = el('pane');
    const location = {
      current: {
        dropTargets: [
          { element: editor, data: { zone: 'editor' } },
          { element: pane, data: { zone: 'pane' } },
        ],
      },
    };
    expect(isInnermostFileTarget(location, editor)).toBe(true);
    expect(isInnermostFileTarget(location, pane)).toBe(false);
  });

  it('skips non-zone targets (e.g. unrelated drop targets in the stack)', () => {
    const pane = el('pane');
    const location = {
      current: {
        dropTargets: [
          { element: el('other'), data: {} },
          { element: pane, data: { zone: 'pane' } },
        ],
      },
    };
    expect(isInnermostFileTarget(location, pane)).toBe(true);
  });
});
