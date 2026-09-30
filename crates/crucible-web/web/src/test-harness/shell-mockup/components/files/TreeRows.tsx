/**
 * The folders of one tree level by name, then its files in the chosen
 * order. An open folder shows its level under it, over a faint guide line
 * in the other shade, as in Obsidian.
 */
import { For, Show, type Component } from 'solid-js';
import { basename } from '../path';
import { DirRow } from './DirRow';
import { FileRow } from './FileRow';
import type { FileLabels } from './fileLabel';
import type { TreeNode } from './tree';

export interface TreeRowsProps {
  node: TreeNode;
  depth: number;
  labels: FileLabels;
  /** The order of the files of one folder. Folders keep the name order. */
  compareFiles: (a: string, b: string) => number;
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
            <div class="mk-tchildren" style={{ '--mk-depth': props.depth }}>
              <TreeRows {...props} node={d} depth={props.depth + 1} />
            </div>
          </Show>
        </>
      )}
    </For>
    <For each={[...props.node.files].sort(props.compareFiles)}>
      {(f) => (
        <FileRow
          name={basename(f)}
          depth={props.depth}
          labels={props.labels}
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
