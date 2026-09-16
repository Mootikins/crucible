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
 * (`'folder' | 'tree-root' | 'pane' | 'editor'`) and only acts
 * when it is the INNERMOST file target of the drop (pragmatic fires onDrop on
 * the whole target stack; without the innermost check, dropping on an editor
 * would also "open in pane" on the pane behind it).
 */
import {
  draggable,
  dropTargetForElements,
} from '@atlaskit/pragmatic-drag-and-drop/element/adapter';
import { combine } from '@atlaskit/pragmatic-drag-and-drop/combine';
import { isMarkdownPath, noteStem } from './markdown-path';
import { openFileInGroup } from './file-actions';
import { findEdgePanelForGroup, windowActions, windowStore } from '@/stores/windowStore';
import { collectPanes, DROP_OVER_ATTR } from '@/windowing';

type FileDropZone = 'folder' | 'tree-root' | 'pane' | 'editor';

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
  // Both sides ask the same predicate: they used to disagree (target took
  // `.markdown`, source only `.md`), so a `.markdown` note dropped into a note
  // inserted a bare path. The stem must be stripped by the same module too, or
  // the widened source side emits `[[Reading List.markdown]]`.
  const targetIsMd = isMarkdownPath(targetPath);
  const sourceIsMdNote = source.rootKind === 'kiln' && isMarkdownPath(source.name);
  if (targetIsMd && sourceIsMdNote) {
    return `[[${noteStem(source.name)}]]`;
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

/** The pane that shows `groupId`, in the centre or on a rail. */
function paneShowing(groupId: string) {
  const roots = [
    windowStore.layout,
    windowStore.edgePanels.left.layout,
    windowStore.edgePanels.right.layout,
  ];
  return roots.flatMap((root) => collectPanes(root)).find((p) => p.tabGroupId === groupId);
}

/**
 * The window manager's drop target: a file dropped on a pane body or a rail
 * ribbon opens in `groupId`.
 *
 * The drop then shows its result. The pane that holds the group takes focus,
 * and a rail that holds it opens, so a file dropped on a closed rail does not
 * open out of sight. A rail with no group has no place for the file, so a
 * drop with no group does nothing.
 */
export function attachPaneDropTarget(
  element: HTMLElement,
  groupId: () => string | null,
): () => void {
  return attachFileDropTarget(element, {
    zone: 'pane',
    canDrop: (source) => !source.isDir,
    onDragEnter: () => element.setAttribute(DROP_OVER_ATTR, ''),
    onDragLeave: () => element.removeAttribute(DROP_OVER_ATTR),
    onDrop: (source) => {
      const id = groupId();
      if (!id) return;
      const pane = paneShowing(id);
      if (pane) windowActions.setActivePane(pane.id);
      openFileInGroup(id, source.absPath, source.name);
      const rail = findEdgePanelForGroup(id);
      if (rail) windowActions.setEdgePanelCollapsed(rail, false);
    },
  });
}
