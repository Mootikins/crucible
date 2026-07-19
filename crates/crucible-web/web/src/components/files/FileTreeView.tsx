import { Component, For, createSignal, onCleanup, onMount } from 'solid-js';
import {
  TreeView,
  useTreeView,
  type TreeCollection,
  type UseTreeViewReturn,
  type TreeViewLoadChildrenDetails,
  type TreeViewLoadChildrenCompleteDetails,
  type TreeViewExpandedChangeDetails,
  type TreeViewSelectionChangeDetails,
} from '@ark-ui/solid';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import { FileTreeNode, type FileTreeDnd } from './FileTreeNode';
import { attachFileDropTarget, canDropIntoFolder } from '@/lib/file-dnd';
import type { ContextAction } from './FileTreeContextMenu';

/** DOM-id-safe encoding of a relPath (slashes/dots are awkward in ids/selectors). */
export const cssId = (value: string): string =>
  value ? value.replace(/[^a-zA-Z0-9_-]/g, '_') : 'root';

export interface FileTreeViewProps {
  collection: TreeCollection<Node>;
  rootKind: TreeRootKind;
  openFilePath: string | null;
  defaultExpandedValue?: string[];
  /** Project lazy loader; `undefined` for kilns (whole tree pre-built). */
  loadChildren?: (details: TreeViewLoadChildrenDetails<Node>) => Promise<Node[]>;
  /**
   * REQUIRED with `loadChildren`: the collection is controlled, so the machine
   * hands back the merged tree here and the owner must persist it — without
   * this, lazily loaded children are silently discarded (branch expands
   * empty).
   */
  onLoadedTree?: (rootNode: Node) => void;
  /** Opening a leaf routes through selection (one path for mouse AND keyboard). */
  onOpenLeaf: (node: Node) => void;
  onExpandedChange?: (expandedValue: string[]) => void;
  onContextAction: (action: ContextAction, node: Node) => void;
  /** Hand the live machine api to the parent (toolbar: collapse-all, reveal). */
  apiRef?: (api: UseTreeViewReturn<Node>) => void;
  /** Drag-and-drop wiring (absent → tree is drag-inert, e.g. in tests). */
  dnd?: FileTreeDnd;
}

/**
 * `TreeView` shell. Selection is single; opening a leaf is routed exclusively
 * through `onSelectionChange` so mouse and keyboard share one code path (avoids
 * the routing-seam duplicate-side-effect bug class). Branches never "open" a
 * file — they expand (via `expandOnClick`). `canRename` is a no-op in Phase 1
 * (F2 reserved for Phase 2).
 */
export const FileTreeView: Component<FileTreeViewProps> = (props) => {
  const handleSelection = (d: TreeViewSelectionChangeDetails<Node>) => {
    const node = d.selectedNodes[0];
    if (node && !node.isDir) props.onOpenLeaf(node);
  };

  const api = useTreeView<Node>(() => ({
    collection: props.collection,
    selectionMode: 'single',
    expandOnClick: true,
    typeahead: true,
    defaultExpandedValue: props.defaultExpandedValue,
    loadChildren: props.loadChildren,
    onLoadChildrenComplete: (d: TreeViewLoadChildrenCompleteDetails<Node>) =>
      props.onLoadedTree?.(d.collection.rootNode),
    canRename: () => false,
    ids: { node: (v: string) => `filetree-node-${cssId(v)}` },
    onSelectionChange: handleSelection,
    onExpandedChange: (d: TreeViewExpandedChangeDetails<Node>) =>
      props.onExpandedChange?.(d.expandedValue),
  }));

  onMount(() => props.apiRef?.(api));

  // The tree surface itself is a move-to-root target (dropping on empty space
  // below the rows). Folder rows sit above it in the target stack, so the
  // innermost-zone protocol in file-dnd keeps this from stealing their drops.
  const [rootDropOver, setRootDropOver] = createSignal(false);
  const attachRootDrop = (el: HTMLElement) => {
    const dnd = props.dnd;
    if (!dnd) return;
    const cleanup = attachFileDropTarget(el, {
      zone: 'tree-root',
      canDrop: (source) => canDropIntoFolder(source, { rootId: dnd.rootId, relPath: '' }),
      onDragEnter: () => setRootDropOver(true),
      onDragLeave: () => setRootDropOver(false),
      onDrop: (source) => {
        setRootDropOver(false);
        dnd.onMove(source, '');
      },
    });
    onCleanup(cleanup);
  };

  return (
    <TreeView.RootProvider value={api}>
      <TreeView.Tree
        aria-label="File tree"
        ref={attachRootDrop}
        data-file-drop={rootDropOver() ? 'true' : undefined}
        class="px-1 min-h-full data-[file-drop=true]:bg-primary/5"
      >
        <For each={api().collection.rootNode.children}>
          {(node, i) => (
            <FileTreeNode
              node={node}
              indexPath={[i()]}
              rootKind={props.rootKind}
              openFilePath={props.openFilePath}
              onContextAction={props.onContextAction}
              dnd={props.dnd}
            />
          )}
        </For>
      </TreeView.Tree>
    </TreeView.RootProvider>
  );
};
