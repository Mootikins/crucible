/**
 * The bar at the top of the Files pane, as Obsidian's file explorer bar:
 * the root selector on the left; new note, new folder, the sort order and
 * one overflow menu on the right. It belongs to the pane's content, so it
 * shows while the rail's vertical tabs hide the pane's tab bar.
 */
import type { Component } from 'solid-js';
import { ArrowUpNarrowWide, FolderPlus, MoreHorizontal, SquarePen } from '@/lib/icons';
import { IconButton } from '../primitives/IconButton';
import { Menu } from '../primitives/Menu';
import type { FileLabels } from './fileLabel';
import { RootSelector, type RootSelectorProps } from './RootSelector';

export type FileOrder = 'name' | 'modified';

export interface FilesNavBarProps extends RootSelectorProps {
  order: FileOrder;
  onOrder: (order: FileOrder) => void;
  labels: FileLabels;
  onLabels: (labels: FileLabels) => void;
  onNewNote: () => void;
  onNewFolder: () => void;
  onCollapseAll: () => void;
  onRefresh: () => void;
  onReveal: () => void;
}

export const FilesNavBar: Component<FilesNavBarProps> = (props) => (
  <div class="mk-filesbar">
    <RootSelector roots={props.roots} current={props.current} color={props.color} onPick={props.onPick} onManage={props.onManage} />
    <span class="mk-grow" />
    <IconButton label="New note" onClick={() => props.onNewNote()}>
      <SquarePen class="mk-i" />
    </IconButton>
    <IconButton label="New folder" onClick={() => props.onNewFolder()}>
      <FolderPlus class="mk-i" />
    </IconButton>
    <Menu
      label="Change the sort order"
      trigger={<ArrowUpNarrowWide class="mk-i" />}
      align="end"
      entries={[
        { kind: 'item', label: 'File name', checked: props.order === 'name', onSelect: () => props.onOrder('name') },
        { kind: 'item', label: 'Modified time', checked: props.order === 'modified', onSelect: () => props.onOrder('modified') },
      ]}
    />
    <Menu
      label="More"
      trigger={<MoreHorizontal class="mk-i" />}
      align="end"
      entries={[
        { kind: 'item', label: 'Collapse all', onSelect: props.onCollapseAll },
        props.labels === 'icons'
          ? { kind: 'item', label: 'Show extensions', onSelect: () => props.onLabels('extensions') }
          : { kind: 'item', label: 'Show file icons', onSelect: () => props.onLabels('icons') },
        { kind: 'item', label: 'Refresh', onSelect: props.onRefresh },
        { kind: 'item', label: 'Reveal the active file', onSelect: props.onReveal },
        { kind: 'sep' },
        { kind: 'item', label: 'Manage projects and kilns…', onSelect: props.onManage },
      ]}
    />
  </div>
);
