import { Component, For, Show, createMemo, createSignal, onCleanup } from 'solid-js';
import { TreeView } from '@ark-ui/solid';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import {
  attachFileDraggable,
  attachFileDropTarget,
  canDropIntoFolder,
  type FileDragData,
} from '@/lib/file-dnd';
import { FileTreeContextMenu, type ContextAction } from './FileTreeContextMenu';
import { Folder, FolderOpen, ChevronRight } from '@/lib/icons';
import { Dynamic } from 'solid-js/web';
import { fileIconFor } from '@/lib/file-icons';

/** VSCode-style colored filetype icon, resolved by full filename. */
const FileIcon: Component<{ name: string }> = (props) => {
  const meta = createMemo(() => fileIconFor(props.name));
  return (
    <Dynamic
      component={meta().icon}
      class="w-4 h-4 mr-1.5 shrink-0"
      style={{ color: meta().color }}
    />
  );
};

const FolderIcon: Component<{ open?: boolean }> = (props) => (
  <Show when={props.open} fallback={<Folder class="w-4 h-4 mr-1.5 shrink-0" />}>
    <FolderOpen class="w-4 h-4 mr-1.5 shrink-0" />
  </Show>
);

/** DnD wiring handed down from FilesPanel (absent → tree is drag-inert). */
export interface FileTreeDnd {
  rootId: string;
  rootKind: TreeRootKind;
  rootPath: string;
  onMove: (source: FileDragData, destParentRel: string) => void;
  /** Auto-expand a hovered branch (drag lingering over a closed folder). */
  expandBranch: (relPath: string) => void;
}

/** Hover-to-auto-expand delay while dragging over a closed folder. */
const AUTO_EXPAND_MS = 700;

/**
 * Row indentation from zag's `--depth` var (1 = top level). Branch rows
 * show a 14px (0.875rem) chevron before their icon that file rows lack, so
 * file rows take one extra chevron-width of padding — icons (and names)
 * then align within a level.
 */
const DEPTH_INDENT = {
  'padding-left': 'calc(0.5rem + (var(--depth, 1) - 1) * 0.875rem)',
} as const;
const DEPTH_INDENT_LEAF = {
  'padding-left': 'calc(0.5rem + 0.875rem + (var(--depth, 1) - 1) * 0.875rem)',
} as const;

/**
 * Recursive branch/leaf renderer built on ark-ui `TreeView`. The machine emits
 * `role=tree/treeitem/group` and `aria-expanded/selected/level/setsize/posinset`
 * via `getBranchProps`/`getItemProps` — we never hand-author them. We only
 * AUGMENT (never overwrite) with the open-note markers (`aria-current`,
 * `data-current`) when the node's absolute path is the active editor file.
 * Depth indentation consumes the machine's `--depth` CSS var (set by zag on
 * every item/branch-control) via `DEPTH_INDENT` — files sit one chevron-width
 * deeper than their folder so names align.
 */
export const FileTreeNode: Component<{
  node: Node;
  indexPath: number[];
  rootKind: TreeRootKind;
  openFilePath: string | null;
  onContextAction: (action: ContextAction, node: Node) => void;
  dnd?: FileTreeDnd;
}> = (props) => {
  const isCurrent = () => props.node.absPath === props.openFilePath;
  const currentAttrs = () =>
    isCurrent() ? ({ 'aria-current': 'page', 'data-current': 'true' } as const) : {};

  const [dropOver, setDropOver] = createSignal(false);
  const cleanups: Array<() => void> = [];
  onCleanup(() => cleanups.forEach((fn) => fn()));

  const dragData = (): FileDragData => ({
    type: 'fileNode',
    rootId: props.dnd!.rootId,
    rootKind: props.dnd!.rootKind,
    rootPath: props.dnd!.rootPath,
    relPath: props.node.relPath,
    absPath: props.node.absPath,
    name: props.node.name,
    isDir: props.node.isDir,
  });

  /** Ref callback: every row is a drag source. */
  const attachDrag = (el: HTMLElement) => {
    if (!props.dnd) return;
    cleanups.push(attachFileDraggable(el, dragData));
  };

  /** Ref callback: folder rows are also move targets (with auto-expand). */
  const attachFolderDrop = (el: HTMLElement) => {
    attachDrag(el);
    const dnd = props.dnd;
    if (!dnd) return;
    let expandTimer: ReturnType<typeof setTimeout> | null = null;
    const clearTimer = () => {
      if (expandTimer !== null) clearTimeout(expandTimer);
      expandTimer = null;
    };
    cleanups.push(clearTimer);
    cleanups.push(
      attachFileDropTarget(el, {
        zone: 'folder',
        canDrop: (source) =>
          canDropIntoFolder(source, { rootId: dnd.rootId, relPath: props.node.relPath }),
        onDragEnter: () => {
          setDropOver(true);
          expandTimer = setTimeout(() => dnd.expandBranch(props.node.relPath), AUTO_EXPAND_MS);
        },
        onDragLeave: () => {
          setDropOver(false);
          clearTimer();
        },
        onDrop: (source) => {
          setDropOver(false);
          clearTimer();
          dnd.onMove(source, props.node.relPath);
        },
      }),
    );
  };

  return (
    <TreeView.NodeProvider node={props.node} indexPath={props.indexPath}>
      <Show
        when={props.node.isDir}
        fallback={
          <FileTreeContextMenu node={props.node} rootKind={props.rootKind} onAction={props.onContextAction}>
            <TreeView.Item
              {...currentAttrs()}
              ref={attachDrag}
              class="flex items-center pr-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-shell-body text-sm data-[selected]:bg-hover-wash data-[current=true]:font-medium data-[current=true]:border-l-2 data-[current=true]:border-primary"
              style={DEPTH_INDENT_LEAF}
            >
              <FileIcon name={props.node.name} />
              <TreeView.ItemText class="truncate">{props.node.name}</TreeView.ItemText>
              <TreeView.NodeRenameInput class="bg-surface-base text-shell-body text-sm px-1 rounded border border-primary outline-none min-w-0 flex-1" />
            </TreeView.Item>
          </FileTreeContextMenu>
        }
      >
        <TreeView.Branch>
          <FileTreeContextMenu node={props.node} rootKind={props.rootKind} onAction={props.onContextAction}>
            <TreeView.BranchControl
              {...currentAttrs()}
              ref={attachFolderDrop}
              data-file-drop={dropOver() ? 'true' : undefined}
              class="flex items-center pr-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-shell-body text-sm data-[selected]:bg-hover-wash data-[file-drop=true]:bg-primary/15 data-[file-drop=true]:outline data-[file-drop=true]:outline-1 data-[file-drop=true]:outline-primary"
              style={DEPTH_INDENT}
            >
              {/* zag stamps data-state on the INDICATOR, not its children — the
                  rotate selector must live here or it never matches. */}
              <TreeView.BranchIndicator class="shrink-0 transition-transform duration-150 data-[state=open]:rotate-90">
                <ChevronRight class="w-3.5 h-3.5" />
              </TreeView.BranchIndicator>
              <FolderIcon />
              <TreeView.BranchText class="truncate">{props.node.name}</TreeView.BranchText>
              <TreeView.NodeRenameInput class="bg-surface-base text-shell-body text-sm px-1 rounded border border-primary outline-none min-w-0 flex-1" />
            </TreeView.BranchControl>
          </FileTreeContextMenu>
          <TreeView.BranchContent>
            <For each={props.node.children}>
              {(child, i) => (
                <FileTreeNode
                  node={child}
                  indexPath={[...props.indexPath, i()]}
                  rootKind={props.rootKind}
                  openFilePath={props.openFilePath}
                  onContextAction={props.onContextAction}
                  dnd={props.dnd}
                />
              )}
            </For>
          </TreeView.BranchContent>
        </TreeView.Branch>
      </Show>
    </TreeView.NodeProvider>
  );
};
