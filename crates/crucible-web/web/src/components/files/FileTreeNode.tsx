import { Component, For, Index, Show, createMemo, createSignal, onCleanup } from 'solid-js';
import { TreeView } from '@ark-ui/solid';
import { Dynamic } from 'solid-js/web';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import {
  attachFileDraggable,
  attachFileDropTarget,
  canDropIntoFolder,
  type FileDragData,
} from '@/lib/file-dnd';
import { fileIconFor } from '@/lib/file-icons';
import { FileTreeContextMenu, type ContextAction } from './FileTreeContextMenu';
import { ChevronRight } from '@/lib/icons';

/** Colored filetype icon, resolved by full filename — slightly smaller than
 * the folder chevron so files read as lighter than folders. */
const FileIcon: Component<{ name: string }> = (props) => {
  const meta = createMemo(() => fileIconFor(props.name));
  return <Dynamic component={meta().icon} class="w-3.5 h-3.5 shrink-0" style={{ color: meta().color }} />;
};

/**
 * One vertical indent guide per ancestor level (VSCode/Theia idiom). Each
 * guide is a full-height column with a hairline down its middle, so guides at
 * the same depth line up under their parent's chevron and connect across
 * sibling rows. `depth` is 1-based (top-level = 1 → no guides).
 */
const IndentGuides: Component<{ depth: number }> = (props) => (
  <Index each={Array.from({ length: Math.max(0, props.depth - 1) })}>
    {() => (
      <span aria-hidden="true" class="shrink-0 self-stretch" style={{ width: '1rem' }}>
        <span
          class="block h-full"
          style={{ width: '1px', 'margin-left': '0.5rem', background: 'var(--color-hairline)' }}
        />
      </span>
    )}
  </Index>
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

/** Shared row skin: full-height flex so indent guides run edge-to-edge.
 * `group` drives the fade-scroll marquee on the name when the row is hovered. */
const ROW =
  'group flex items-stretch pr-2 rounded cursor-pointer hover:bg-hover-wash text-shell-body text-[13px]';
/** Fixed icon column — the chevron (folders) and filetype icon (files) share
 * it, so names align at a level regardless of node kind. */
const ICON_SLOT = 'w-4 py-1 flex items-center justify-center shrink-0';

/**
 * Recursive branch/leaf renderer built on ark-ui `TreeView`. The machine emits
 * `role=tree/treeitem/group` and `aria-expanded/selected/level/setsize/posinset`
 * via `getBranchProps`/`getItemProps` — we never hand-author them. We only
 * AUGMENT (never overwrite) with the open-note markers (`aria-current`,
 * `data-current`) when the node's absolute path is the active editor file.
 *
 * Indentation is drawn as explicit per-level guide columns (`IndentGuides`)
 * keyed off `indexPath.length` — every row has ONE icon slot at the same
 * offset (rotating chevron for folders, filetype icon for files; no separate
 * folder glyph), so folder and file names align within a level.
 */
export const FileTreeNode: Component<{
  node: Node;
  indexPath: number[];
  rootKind: TreeRootKind;
  openFilePath: string | null;
  onContextAction: (action: ContextAction, node: Node) => void;
  showHidden?: boolean;
  /** Transform a node's display name (e.g. strip hidden extensions). Identity
   * when absent; never changes `node.name` (rename/logic use the real name). */
  formatName?: (name: string) => string;
  dnd?: FileTreeDnd;
}> = (props) => {
  const shown = () => (props.formatName ? props.formatName(props.node.name) : props.node.name);
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
          <FileTreeContextMenu node={props.node} rootKind={props.rootKind} onAction={props.onContextAction} showHidden={props.showHidden}>
            <TreeView.Item
              {...currentAttrs()}
              ref={attachDrag}
              title={props.node.relPath || props.node.name}
              class={`${ROW} relative data-[selected]:bg-hover-wash data-[current=true]:font-medium`}
            >
              <IndentGuides depth={props.indexPath.length} />
              <span class={ICON_SLOT}>
                <FileIcon name={props.node.name} />
              </span>
              {/* fade-scroll masks the overflow with a gradient (no ellipsis
                  glyph to collide with the open-file dot); marquees on hover. */}
              <TreeView.ItemText class="flex-1 min-w-0 fade-scroll py-1 ml-1"><span>{shown()}</span></TreeView.ItemText>
              <TreeView.NodeRenameInput class="bg-surface-base text-shell-body text-sm px-1 my-0.5 rounded border border-primary outline-none min-w-0 flex-1" />
              {/* Open-in-editor marker: an absolute dot so it never shifts the
                  icon / name / indent guides (the old left border did). */}
              <Show when={isCurrent()}>
                <span aria-hidden="true" class="absolute right-1.5 top-1/2 -translate-y-1/2 w-1.5 h-1.5 rounded-full bg-primary" />
              </Show>
            </TreeView.Item>
          </FileTreeContextMenu>
        }
      >
        <TreeView.Branch>
          <FileTreeContextMenu node={props.node} rootKind={props.rootKind} onAction={props.onContextAction} showHidden={props.showHidden}>
            <TreeView.BranchControl
              {...currentAttrs()}
              ref={attachFolderDrop}
              title={props.node.relPath || props.node.name}
              data-file-drop={dropOver() ? 'true' : undefined}
              class={`${ROW} data-[selected]:bg-hover-wash data-[file-drop=true]:bg-primary/15 data-[file-drop=true]:outline data-[file-drop=true]:outline-1 data-[file-drop=true]:outline-primary`}
            >
              <IndentGuides depth={props.indexPath.length} />
              {/* The chevron IS the folder icon (no separate glyph). zag stamps
                  data-state on the INDICATOR, not its children — the rotate
                  selector must live here or it never matches. */}
              <TreeView.BranchIndicator
                class={`${ICON_SLOT} text-muted transition-transform duration-150 data-[state=open]:rotate-90`}
              >
                <ChevronRight class="w-3.5 h-3.5" />
              </TreeView.BranchIndicator>
              <TreeView.BranchText class="flex-1 min-w-0 fade-scroll py-1 ml-1"><span>{shown()}</span></TreeView.BranchText>
              <TreeView.NodeRenameInput class="bg-surface-base text-shell-body text-sm px-1 my-0.5 rounded border border-primary outline-none min-w-0 flex-1" />
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
                  showHidden={props.showHidden}

                  formatName={props.formatName}
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
