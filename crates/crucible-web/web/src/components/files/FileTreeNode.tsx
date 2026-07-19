import { Component, For, Show, createMemo } from 'solid-js';
import { TreeView } from '@ark-ui/solid';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import { FileTreeContextMenu, type ContextAction } from './FileTreeContextMenu';
import {
  FileText,
  FileCode,
  File,
  Folder,
  FolderOpen,
  FileJson,
  Palette,
  Globe,
  Moon,
  Cog,
  ChevronRight,
} from '@/lib/icons';

const getExtension = (filename: string): string => {
  const parts = filename.split('.');
  return parts.length > 1 ? parts[parts.length - 1] : '';
};

const KNOWN_EXTS = ['md', 'ts', 'tsx', 'js', 'jsx', 'rs', 'json', 'toml', 'yaml', 'yml', 'css', 'scss', 'html', 'lua', 'fnl'];

const FileIcon: Component<{ extension: string }> = (props) => {
  const ext = createMemo(() => props.extension.toLowerCase());
  return (
    <>
      {ext() === 'md' && <FileText class="w-4 h-4 mr-1.5 shrink-0" />}
      {(ext() === 'ts' || ext() === 'tsx') && <FileCode class="w-4 h-4 mr-1.5 shrink-0" />}
      {(ext() === 'js' || ext() === 'jsx') && <FileCode class="w-4 h-4 mr-1.5 shrink-0" />}
      {ext() === 'rs' && <FileCode class="w-4 h-4 mr-1.5 shrink-0" />}
      {ext() === 'json' && <FileJson class="w-4 h-4 mr-1.5 shrink-0" />}
      {(ext() === 'toml' || ext() === 'yaml' || ext() === 'yml') && <Cog class="w-4 h-4 mr-1.5 shrink-0" />}
      {(ext() === 'css' || ext() === 'scss') && <Palette class="w-4 h-4 mr-1.5 shrink-0" />}
      {ext() === 'html' && <Globe class="w-4 h-4 mr-1.5 shrink-0" />}
      {(ext() === 'lua' || ext() === 'fnl') && <Moon class="w-4 h-4 mr-1.5 shrink-0" />}
      {!KNOWN_EXTS.includes(ext()) && <File class="w-4 h-4 mr-1.5 shrink-0" />}
    </>
  );
};

const FolderIcon: Component<{ open?: boolean }> = (props) => (
  <Show when={props.open} fallback={<Folder class="w-4 h-4 mr-1.5 shrink-0" />}>
    <FolderOpen class="w-4 h-4 mr-1.5 shrink-0" />
  </Show>
);

/**
 * Recursive branch/leaf renderer built on ark-ui `TreeView`. The machine emits
 * `role=tree/treeitem/group` and `aria-expanded/selected/level/setsize/posinset`
 * via `getBranchProps`/`getItemProps` — we never hand-author them. We only
 * AUGMENT (never overwrite) with the open-note markers (`aria-current`,
 * `data-current`) when the node's absolute path is the active editor file.
 * Depth indentation is driven by the machine's `NodeState.depth`.
 */
export const FileTreeNode: Component<{
  node: Node;
  indexPath: number[];
  rootKind: TreeRootKind;
  openFilePath: string | null;
  onContextAction: (action: ContextAction, node: Node) => void;
}> = (props) => {
  const isCurrent = () => props.node.absPath === props.openFilePath;
  const currentAttrs = () =>
    isCurrent() ? ({ 'aria-current': 'page', 'data-current': 'true' } as const) : {};

  return (
    <TreeView.NodeProvider node={props.node} indexPath={props.indexPath}>
      <Show
        when={props.node.isDir}
        fallback={
          <FileTreeContextMenu node={props.node} rootKind={props.rootKind} onAction={props.onContextAction}>
            <TreeView.Item
              {...currentAttrs()}
              class="flex items-center px-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-shell-body text-sm data-[selected]:bg-hover-wash data-[current=true]:font-medium data-[current=true]:border-l-2 data-[current=true]:border-primary"
            >
              <FileIcon extension={getExtension(props.node.name)} />
              <TreeView.ItemText class="truncate">{props.node.name}</TreeView.ItemText>
            </TreeView.Item>
          </FileTreeContextMenu>
        }
      >
        <TreeView.Branch>
          <FileTreeContextMenu node={props.node} rootKind={props.rootKind} onAction={props.onContextAction}>
            <TreeView.BranchControl
              {...currentAttrs()}
              class="flex items-center px-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-shell-body text-sm data-[selected]:bg-hover-wash"
            >
              <TreeView.BranchIndicator class="shrink-0">
                <ChevronRight class="w-3.5 h-3.5 transition-transform data-[state=open]:rotate-90" />
              </TreeView.BranchIndicator>
              <FolderIcon />
              <TreeView.BranchText class="truncate">{props.node.name}</TreeView.BranchText>
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
                />
              )}
            </For>
          </TreeView.BranchContent>
        </TreeView.Branch>
      </Show>
    </TreeView.NodeProvider>
  );
};
