/**
 * The contents of one root: the kiln's folders and files, the top level of
 * the project, or the session's own folder. The root selector over the tree
 * chooses the root.
 */
import { For, Match, Switch, type Component } from 'solid-js';
import { scrollFade } from '../primitives/scrollFade';
import { StaticRow } from './StaticRow';
import { TreeRows, type TreeRowsProps } from './TreeRows';
import type { TreeNode } from './tree';

/** The three roots. The real app lists kilns with `useKilns` and projects with `useProjects`. */
export type RootKey = 'docs' | 'crucible' | 'folder';

export interface FileTreeProps extends Omit<TreeRowsProps, 'node' | 'depth'> {
  root: RootKey;
  kiln: TreeNode;
  /** The top level of the project. A trailing `/` marks a folder. */
  projectEntries: readonly string[];
}

export const FileTree: Component<FileTreeProps> = (props) => (
  <div class="mk-scroll mk-tree" ref={scrollFade('y')}>
    <Switch>
      <Match when={props.root === 'docs'}>
        <TreeRows {...props} node={props.kiln} depth={0} />
      </Match>
      <Match when={props.root === 'crucible'}>
        <For each={props.projectEntries}>{(p) => <StaticRow name={p.replace(/\/$/, '')} dir={p.endsWith('/')} />}</For>
      </Match>
      <Match when={props.root === 'folder'}>
        <StaticRow name="No files yet" quiet />
      </Match>
    </Switch>
  </div>
);
