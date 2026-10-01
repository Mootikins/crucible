import { type Component, type ComponentProps, Show } from 'solid-js';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { ArrowUpDown, SquarePen, FolderPlus, MoreHorizontal, Link2, GitCompare } from '@/lib/icons';
import { menuContent, menuItem, menuTrigger } from '@/components/ui/menu-style';
import type { SortSpec } from '@/lib/file-tree/types';
import type { SessionRoot } from '@/lib/session-roots';
import { RootDropdown } from './RootDropdown';

/** Root navigation and file intents; filesystem state stays in FilesPanel. */
export const FilesToolbar: Component<{
  ownRoots: SessionRoot[];
  groups: ComponentProps<typeof RootDropdown>['groups'];
  activeRoot: SessionRoot | null;
  selectedKey: string | null;
  rootBarRef: (el: HTMLDivElement) => void;
  sort: SortSpec;
  hideExtensions: boolean;
  hasOpenFile: boolean;
  onSelectRoot: ComponentProps<typeof RootDropdown>['onSelect'];
  onNotice: (notice: string | null) => void;
  onAttachRoot: (root: SessionRoot) => void;
  onSort: (key: SortSpec['key']) => void;
  onNewNote: () => void;
  onNewFolder: () => void;
  onCollapseAll: () => void;
  onToggleExtensions: () => void;
  onEditExtensions: () => void;
  onRefresh: () => void;
  onRevealActive: () => void;
  onManageRoots: () => void;
  onBranchDiff: () => void;
}> = (props) => (
  <div class="files-toolbar shrink-0 flex items-center justify-between gap-2 p-3">
    <div class="flex items-center gap-1 min-w-0 flex-1" ref={props.rootBarRef}>
      <RootDropdown
        own={props.ownRoots}
        groups={props.groups}
        selectedKey={props.selectedKey}
        onSelect={props.onSelectRoot}
        activeRoot={props.activeRoot}
        onNotice={props.onNotice}
      />

      {/* Only on the ACTIVE unattached kiln: an affordance for a root you
          are not looking at would be a claim about a corpus you cannot
          see. */}
      <Show when={props.activeRoot?.origin === 'other-kiln' ? props.activeRoot : null} keyed>
        {(root) => (
          <button
            type="button"
            data-testid="root-attach"
            title={`Let this session query ${root.name}`}
            onClick={() => props.onAttachRoot(root)}
            class="flex items-center gap-1 px-1.5 py-1 rounded-control text-floor text-muted hover:text-shell-ink hover:bg-hover-wash whitespace-nowrap transition-colors"
          >
            <Link2 class="w-3 h-3" /> Attach
          </button>
        )}
      </Show>
    </div>
    <div class="flex items-center gap-1 shrink-0">
      <Show when={props.activeRoot}>
        <button
          type="button"
          aria-label="New note"
          title="New note"
          onClick={() => {
            props.onNewNote();
          }}
          class="p-1 rounded-control hover:bg-hover-wash text-muted"
        >
          <SquarePen class="w-3.5 h-3.5" />
        </button>
      </Show>
      <button type="button" aria-label="New folder" title="New folder"
        disabled={!props.activeRoot} class={menuTrigger}
        onClick={() => { props.onNewFolder(); }}>
        <FolderPlus class="w-3.5 h-3.5" />
      </button>
      <Menu.Root onSelect={({ value }) => props.onSort(value as SortSpec['key'])}>
        <Menu.Trigger aria-label="Sort" title="Change the sort order" class={menuTrigger}>
          <ArrowUpDown class="w-3.5 h-3.5" />
        </Menu.Trigger>
        <Portal><Menu.Positioner><Menu.Content class={menuContent}>
          <Menu.Item value="name" class={menuItem}>File name {props.sort.key === 'name' ? '✓' : ''}</Menu.Item>
          <Menu.Item value="modified" class={menuItem}>Modified time {props.sort.key === 'modified' ? '✓' : ''}</Menu.Item>
        </Menu.Content></Menu.Positioner></Portal>
      </Menu.Root>
      <Menu.Root onSelect={({ value }) => {
        if (value === 'collapse') props.onCollapseAll();
        if (value === 'extensions') props.onToggleExtensions();
        if (value === 'extension-list') props.onEditExtensions();
        if (value === 'refresh') { props.onRefresh(); }
        if (value === 'reveal') props.onRevealActive();
        if (value === 'manage') props.onManageRoots();
      }}>
        <Menu.Trigger aria-label="More file actions" title="More file actions" class={menuTrigger}>
          <MoreHorizontal class="w-3.5 h-3.5" />
        </Menu.Trigger>
        <Portal><Menu.Positioner><Menu.Content class={menuContent}>
          <Menu.Item value="collapse" class={menuItem}>Collapse all</Menu.Item>
          <Menu.Item value="extensions" class={menuItem}>{props.hideExtensions ? 'Show extensions' : 'Hide extensions'}</Menu.Item>
          <Menu.Item value="extension-list" class={menuItem}>Choose hidden extensions…</Menu.Item>
          <Menu.Item value="refresh" disabled={!props.activeRoot} class={menuItem}>Refresh</Menu.Item>
          <Menu.Item value="reveal" disabled={!props.hasOpenFile} class={menuItem}>Reveal the active file</Menu.Item>
          <Menu.Item value="manage" class={menuItem}>Manage projects and kilns…</Menu.Item>
        </Menu.Content></Menu.Positioner></Portal>
      </Menu.Root>
      <Show when={props.activeRoot?.git}>
        <button
          type="button"
          aria-label="Open branch diff"
          title="Open branch diff"
          data-testid="open-branch-diff"
          onClick={() => props.onBranchDiff()}
          class="p-1 rounded-control hover:bg-hover-wash text-muted"
        >
          <GitCompare class="w-3.5 h-3.5" />
        </button>
      </Show>

    </div>
  </div>
);
