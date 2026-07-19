import { Component, For, JSX, Show } from 'solid-js';
import { Menu } from '@ark-ui/solid';
import type { FileTreeNode } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import { FileText, Target, Copy, RefreshCw, Pencil, Plus, FolderTree, Trash2 } from '@/lib/icons';

/**
 * Context menu action model. Read actions plus the Phase-2 mutations, now
 * rendered: `rename` (link-safe via daemon note.rename), `new-note` (kiln
 * folders — the only root kind with a write API), `new-folder` (fs.mkdir),
 * `delete` (fs.trash → `.crucible/trash/`). DnD covers `move`, so it has no
 * menu entry.
 */
export type ContextAction =
  | 'open'
  | 'copy-path'
  | 'copy-relative-path'
  | 'reveal-in-tree'
  | 'refresh'
  | 'new-note'
  | 'new-folder'
  | 'rename'
  | 'delete';

export interface ContextItem {
  action: ContextAction;
  label: string;
  icon: Component<{ class?: string }>;
  enabledFor: 'file' | 'dir' | 'both';
  /** Root kinds the action is available for ('both' when absent). */
  kinds?: TreeRootKind;
  /** Render a separator ABOVE this item (read vs mutate grouping). */
  group?: boolean;
  danger?: boolean;
}

export const CONTEXT_ITEMS: ContextItem[] = [
  { action: 'open', label: 'Open', icon: FileText, enabledFor: 'file' },
  { action: 'reveal-in-tree', label: 'Reveal in tree', icon: Target, enabledFor: 'both' },
  { action: 'copy-path', label: 'Copy path', icon: Copy, enabledFor: 'both' },
  { action: 'copy-relative-path', label: 'Copy relative path', icon: Copy, enabledFor: 'both' },
  { action: 'refresh', label: 'Refresh', icon: RefreshCw, enabledFor: 'dir', kinds: 'project' },
  { action: 'new-note', label: 'New note', icon: Plus, enabledFor: 'dir', kinds: 'kiln', group: true },
  { action: 'new-folder', label: 'New folder', icon: FolderTree, enabledFor: 'dir' },
  { action: 'rename', label: 'Rename', icon: Pencil, enabledFor: 'both', group: true },
  { action: 'delete', label: 'Delete', icon: Trash2, enabledFor: 'both', danger: true },
];

const matchesKind = (item: ContextItem, isDir: boolean): boolean =>
  item.enabledFor === 'both' || (isDir ? item.enabledFor === 'dir' : item.enabledFor === 'file');

/**
 * Pure: the menu items to render for a node. `refresh` is project-only (kiln
 * roots are live via SSE); `new-note` is kiln-only (projects have no write
 * API — read-only by design).
 */
export function itemsForNode(node: FileTreeNode, rootKind: TreeRootKind): ContextItem[] {
  return CONTEXT_ITEMS.filter(
    (i) => matchesKind(i, node.isDir) && (i.kinds === undefined || i.kinds === rootKind),
  );
}

/**
 * Wraps a row (`children`) in an ark-ui context trigger. Right-click or
 * Shift+F10 opens a `role=menu` with keyboard support; selecting an item routes
 * through `onAction`.
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
        {/* asChild div (display:contents): the default trigger is a BUTTON,
            and tree rows contain focusables (rename input) — interactive
            content inside a button is invalid and steals keys. */}
        <Menu.ContextTrigger
          asChild={(triggerProps) => (
            <div {...triggerProps({ class: 'contents' })}>{props.children}</div>
          )}
        />
        <Menu.Positioner>
          <Menu.Content class="min-w-[10rem] rounded border border-hairline bg-surface-elevated py-1 text-xs text-shell-ink shadow-lg focus:outline-none">
            <For each={items()}>
              {(item) => (
                <>
                  <Show when={item.group}>
                    <Menu.Separator class="my-1 border-t border-hairline" />
                  </Show>
                  <Menu.Item
                    value={item.action}
                    class="flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash"
                    classList={{ 'text-error': item.danger === true }}
                  >
                    <item.icon class="w-3.5 h-3.5 shrink-0" />
                    <span>{item.label}</span>
                  </Menu.Item>
                </>
              )}
            </For>
          </Menu.Content>
        </Menu.Positioner>
      </Menu.Root>
    </Show>
  );
};
