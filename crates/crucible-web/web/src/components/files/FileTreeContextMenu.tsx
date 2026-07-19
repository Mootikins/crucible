import { Component, For, JSX, Show } from 'solid-js';
import { Menu } from '@ark-ui/solid';
import type { FileTreeNode } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import { FileText, Target, Copy, RefreshCw, Pencil, Plus, FolderTree, Trash2 } from '@/lib/icons';

/**
 * Read-only-in-Phase-1 context menu action model. Phase-2 mutations
 * (`new-note`, `rename`, `move`, `delete`, …) are declared here so they slot in
 * through the same `onContextAction` seam, but they are `phase: 2` and NEVER
 * rendered in Phase 1.
 */
export type ContextAction =
  // P1 read-only
  | 'open'
  | 'copy-path'
  | 'copy-relative-path'
  | 'reveal-in-tree'
  | 'refresh'
  // P2 seam (declared, never rendered in P1)
  | 'new-note'
  | 'new-folder'
  | 'rename'
  | 'move'
  | 'delete';

export interface ContextItem {
  action: ContextAction;
  label: string;
  icon: Component<{ class?: string }>;
  phase: 1 | 2;
  enabledFor: 'file' | 'dir' | 'both';
}

export const CONTEXT_ITEMS: ContextItem[] = [
  { action: 'open', label: 'Open', icon: FileText, phase: 1, enabledFor: 'file' },
  { action: 'reveal-in-tree', label: 'Reveal in tree', icon: Target, phase: 1, enabledFor: 'both' },
  { action: 'copy-path', label: 'Copy path', icon: Copy, phase: 1, enabledFor: 'both' },
  { action: 'copy-relative-path', label: 'Copy relative path', icon: Copy, phase: 1, enabledFor: 'both' },
  { action: 'refresh', label: 'Refresh', icon: RefreshCw, phase: 1, enabledFor: 'dir' },
  // ---- Phase 2 seam (hidden in P1) ----
  { action: 'new-note', label: 'New note', icon: Plus, phase: 2, enabledFor: 'dir' },
  { action: 'new-folder', label: 'New folder', icon: FolderTree, phase: 2, enabledFor: 'dir' },
  { action: 'rename', label: 'Rename', icon: Pencil, phase: 2, enabledFor: 'both' },
  { action: 'move', label: 'Move', icon: FolderTree, phase: 2, enabledFor: 'both' },
  { action: 'delete', label: 'Delete', icon: Trash2, phase: 2, enabledFor: 'both' },
];

const matchesKind = (item: ContextItem, isDir: boolean): boolean =>
  item.enabledFor === 'both' || (isDir ? item.enabledFor === 'dir' : item.enabledFor === 'file');

/**
 * Pure: the menu items to render for a node. Phase-1 only; `refresh` is a
 * project-only interaction (kiln roots are live via SSE, so it is hidden for
 * them).
 */
export function itemsForNode(node: FileTreeNode, rootKind: TreeRootKind): ContextItem[] {
  return CONTEXT_ITEMS.filter(
    (i) =>
      i.phase === 1 &&
      matchesKind(i, node.isDir) &&
      !(i.action === 'refresh' && rootKind === 'kiln'),
  );
}

/**
 * Wraps a row (`children`) in an ark-ui context trigger. Right-click or
 * Shift+F10 opens a `role=menu` with keyboard support; selecting an item routes
 * through `onAction`. All Phase-2 entries are filtered out.
 */
export const FileTreeContextMenu: Component<{
  node: FileTreeNode;
  rootKind: TreeRootKind;
  onAction: (action: ContextAction, node: FileTreeNode) => void;
  children: JSX.Element;
}> = (props) => {
  const items = () => itemsForNode(props.node, props.rootKind);
  return (
    <Show when={items().length > 0} fallback={props.children}>
      <Menu.Root onSelect={(d) => props.onAction(d.value as ContextAction, props.node)}>
        <Menu.ContextTrigger>{props.children}</Menu.ContextTrigger>
        <Menu.Positioner>
          <Menu.Content class="min-w-[10rem] rounded border border-hairline bg-surface-elevated py-1 text-xs text-shell-ink shadow-lg focus:outline-none">
            <For each={items()}>
              {(item) => (
                <Menu.Item
                  value={item.action}
                  class="flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash"
                >
                  <item.icon class="w-3.5 h-3.5 shrink-0" />
                  <span>{item.label}</span>
                </Menu.Item>
              )}
            </For>
          </Menu.Content>
        </Menu.Positioner>
      </Menu.Root>
    </Show>
  );
};
