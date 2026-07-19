import { Component, For, onMount } from 'solid-js';
import {
  TreeView,
  useTreeView,
  type TreeCollection,
  type UseTreeViewReturn,
  type TreeViewLoadChildrenDetails,
  type TreeViewExpandedChangeDetails,
  type TreeViewSelectionChangeDetails,
} from '@ark-ui/solid';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import { FileTreeNode } from './FileTreeNode';
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
  /** Opening a leaf routes through selection (one path for mouse AND keyboard). */
  onOpenLeaf: (node: Node) => void;
  onExpandedChange?: (expandedValue: string[]) => void;
  onContextAction: (action: ContextAction, node: Node) => void;
  /** Hand the live machine api to the parent (toolbar: collapse-all, reveal). */
  apiRef?: (api: UseTreeViewReturn<Node>) => void;
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
    canRename: () => false,
    ids: { node: (v: string) => `filetree-node-${cssId(v)}` },
    onSelectionChange: handleSelection,
    onExpandedChange: (d: TreeViewExpandedChangeDetails<Node>) =>
      props.onExpandedChange?.(d.expandedValue),
  }));

  onMount(() => props.apiRef?.(api));

  return (
    <TreeView.RootProvider value={api}>
      <TreeView.Tree aria-label="File tree" class="px-1">
        <For each={api().collection.rootNode.children}>
          {(node, i) => (
            <FileTreeNode
              node={node}
              indexPath={[i()]}
              rootKind={props.rootKind}
              openFilePath={props.openFilePath}
              onContextAction={props.onContextAction}
            />
          )}
        </For>
      </TreeView.Tree>
    </TreeView.RootProvider>
  );
};
