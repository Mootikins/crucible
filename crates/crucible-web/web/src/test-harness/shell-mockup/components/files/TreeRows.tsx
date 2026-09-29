/** The folders of one tree level, then its notes, both by name; an open folder shows its level under it. */
import { For, Show, type Component } from 'solid-js';
import { basename } from '../path';
import { DirRow } from './DirRow';
import { FileRow } from './FileRow';
import type { TreeNode } from './tree';

export interface TreeRowsProps {
  node: TreeNode;
  depth: number;
  isOpen: (dirPath: string) => boolean;
  onToggle: (dirPath: string) => void;
  activePath?: string;
  pendingFor: (path: string) => number;
  touched: (path: string) => boolean;
  color: string;
  onOpen: (path: string, e: MouseEvent) => void;
}

export const TreeRows: Component<TreeRowsProps> = (props) => (
  <>
    <For each={[...props.node.dirs.values()].sort((a, b) => a.name.localeCompare(b.name))}>
      {(d) => (
        <>
          <DirRow name={d.name} depth={props.depth} open={props.isOpen(d.path)} onToggle={() => props.onToggle(d.path)} />
          <Show when={props.isOpen(d.path)}>
            <TreeRows {...props} node={d} depth={props.depth + 1} />
          </Show>
        </>
      )}
    </For>
    <For each={[...props.node.files].sort((a, b) => basename(a).localeCompare(basename(b)))}>
      {(f) => (
        <FileRow
          name={basename(f)}
          depth={props.depth}
          current={props.activePath === f}
          pending={props.pendingFor(f)}
          touched={props.touched(f)}
          color={props.color}
          onOpen={(e) => props.onOpen(f, e)}
        />
      )}
    </For>
  </>
);
