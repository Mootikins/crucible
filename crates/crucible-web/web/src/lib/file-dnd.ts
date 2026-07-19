/**
 * File-tree drag-and-drop glue over @atlaskit/pragmatic-drag-and-drop.
 *
 * Native-HTML5 drags (not @thisbeyond/solid-dnd): file drags must cross
 * surfaces — tree → editor pane (open), tree → CodeMirror content (insert
 * link), tree → folder (move) — and solid-dnd only matches within its nearest
 * `DragDropProvider`, which is exactly what the windowing layer owns. The two
 * systems coexist: solid-dnd is pointer-event based, pragmatic is native
 * dragstart/drop, so tab drags and file drags never see each other.
 *
 * Zone protocol: every file-accepting drop target tags its data with a `zone`
 * (`'folder' | 'tree-root' | 'pane' | 'editor'`) and only acts when it is the
 * INNERMOST file target of the drop (pragmatic fires onDrop on the whole
 * target stack; without the innermost check, dropping on an editor would also
 * "open in pane" on the pane behind it).
 */
import {
  draggable,
  dropTargetForElements,
} from '@atlaskit/pragmatic-drag-and-drop/element/adapter';
import { combine } from '@atlaskit/pragmatic-drag-and-drop/combine';

export type FileDropZone = 'folder' | 'tree-root' | 'pane' | 'editor';

/** Payload attached to a file-tree node drag. Identity is rootKey + relPath. */
export type FileDragData = {
  type: 'fileNode';
  /** `rootKey(root)` — kind-qualified, so a project and same-path kiln differ. */
  rootId: string;
  rootKind: 'project' | 'kiln';
  /** Absolute root path (what fs.move takes as `root`). */
  rootPath: string;
  relPath: string;
  absPath: string;
  name: string;
  isDir: boolean;
};

export function isFileDragData(data: Record<string, unknown>): data is FileDragData {
  return data.type === 'fileNode';
}

/**
 * Move-legality guard shared by canDrop and the drop handler: same root only
 * (fs.move is within-root), never into the node's own parent (no-op → daemon
 * would reject as existing destination), never a dir into itself or its own
 * descendants.
 */
export function canDropIntoFolder(
  source: FileDragData,
  dest: { rootId: string; relPath: string },
): boolean {
  if (source.rootId !== dest.rootId) return false;
  const parentOf = (rel: string) => (rel.includes('/') ? rel.slice(0, rel.lastIndexOf('/')) : '');
  if (parentOf(source.relPath) === dest.relPath) return false; // already there
  if (!source.isDir) return true;
  return source.relPath !== dest.relPath && !dest.relPath.startsWith(source.relPath + '/');
}

/** Destination rel-path for a move into `destParentRel` (`''` = the root). */
export function moveTargetRel(source: FileDragData, destParentRel: string): string {
  return destParentRel ? `${destParentRel}/${source.name}` : source.name;
}

/**
 * Text a file drop inserts into an editor: kiln markdown notes dropped into a
 * markdown file become wikilinks (Obsidian convention, resolved by stem);
 * everything else inserts the root-relative path.
 */
export function insertTextFor(source: FileDragData, targetPath: string): string {
  const targetIsMd = /\.(md|markdown)$/i.test(targetPath);
  const sourceIsMdNote = source.rootKind === 'kiln' && /\.md$/i.test(source.name);
  if (targetIsMd && sourceIsMdNote) {
    return `[[${source.name.replace(/\.md$/i, '')}]]`;
  }
  return source.relPath;
}

/** True when `element` is the innermost file-accepting target of this drop. */
export function isInnermostFileTarget(
  location: { current: { dropTargets: Array<{ element: Element; data: Record<string, unknown> }> } },
  element: Element,
): boolean {
  const innermost = location.current.dropTargets.find((t) => typeof t.data.zone === 'string');
  return innermost?.element === element;
}

/** Attach a file drag source. Returns the cleanup fn (call in onCleanup). */
export function attachFileDraggable(element: HTMLElement, getData: () => FileDragData): () => void {
  return draggable({
    element,
    getInitialData: () => getData(),
  });
}

export interface FileDropTargetOptions {
  zone: FileDropZone;
  /** Reject drags this target can't take (beyond the fileNode type check). */
  canDrop?: (source: FileDragData) => boolean;
  onDragEnter?: (source: FileDragData) => void;
  onDragLeave?: () => void;
  /** Fires only when this target is the INNERMOST file target of the drop. */
  onDrop?: (
    source: FileDragData,
    input: { clientX: number; clientY: number },
  ) => void;
}

/** Attach a file drop target with the zone/innermost protocol. */
export function attachFileDropTarget(
  element: HTMLElement,
  opts: FileDropTargetOptions,
): () => void {
  return combine(
    dropTargetForElements({
      element,
      getData: () => ({ zone: opts.zone }),
      canDrop: ({ source }) =>
        isFileDragData(source.data) && (opts.canDrop?.(source.data) ?? true),
      onDragEnter: ({ source }) => {
        if (isFileDragData(source.data)) opts.onDragEnter?.(source.data);
      },
      onDragLeave: () => opts.onDragLeave?.(),
      onDrop: ({ source, location }) => {
        opts.onDragLeave?.();
        if (!isFileDragData(source.data)) return;
        if (!isInnermostFileTarget(location, element)) return;
        opts.onDrop?.(source.data, {
          clientX: location.current.input.clientX,
          clientY: location.current.input.clientY,
        });
      },
    }),
  );
}
