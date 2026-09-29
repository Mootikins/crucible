/**
 * The files rail: the kiln, the project and the session's own folder, each
 * under its root row. Which rows are open is display state, so the tree
 * keeps it itself.
 */
import { For, Show, createSignal, type Component } from 'solid-js';
import { StaticRow } from './StaticRow';
import { TreeRoot } from './TreeRoot';
import { TreeRows } from './TreeRows';
import type { TreeNode } from './tree';

/** The three roots. The real app lists kilns with `useKilns` and projects with `useProjects`. */
export type RootKey = 'docs' | 'crucible' | 'folder';

export interface FileTreeProps {
  kiln: TreeNode;
  /** The top level of the project. A trailing `/` marks a folder. */
  projectEntries: readonly string[];
  /** Is this root one of the active session's roots? */
  attached: (root: RootKey) => boolean;
  /** The active session's identity colour. */
  color: string;
  /** The note that shows in the focused tab. */
  activePath?: string;
  pendingFor: (path: string) => number;
  touched: (path: string) => boolean;
  onOpen: (path: string, e: MouseEvent) => void;
}

export const FileTree: Component<FileTreeProps> = (props) => {
  const [open, setOpen] = createSignal(new Set(['docs', 'docs:Help', 'docs:Help/Concepts']));
  const toggle = (key: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });
  return (
    <div class="mk-scroll mk-tree">
      <TreeRoot
        label="docs"
        kind="kiln"
        open={open().has('docs')}
        attached={props.attached('docs')}
        color={props.color}
        onToggle={() => toggle('docs')}
      >
        <TreeRows
          node={props.kiln}
          depth={0}
          isOpen={(dir) => open().has(`docs:${dir}`)}
          onToggle={(dir) => toggle(`docs:${dir}`)}
          activePath={props.activePath}
          pendingFor={props.pendingFor}
          touched={props.touched}
          color={props.color}
          onOpen={props.onOpen}
        />
      </TreeRoot>
      <TreeRoot
        label="crucible"
        kind="project"
        open={open().has('crucible')}
        attached={props.attached('crucible')}
        color={props.color}
        onToggle={() => toggle('crucible')}
      >
        <For each={props.projectEntries}>{(p) => <StaticRow name={p.replace(/\/$/, '')} dir={p.endsWith('/')} />}</For>
      </TreeRoot>
      <Show when={props.attached('folder')}>
        <TreeRoot
          label="Session folder"
          kind="workspace"
          open={open().has('folder')}
          attached={props.attached('folder')}
          color={props.color}
          onToggle={() => toggle('folder')}
        >
          <StaticRow name="No files yet" quiet />
        </TreeRoot>
      </Show>
    </div>
  );
};
