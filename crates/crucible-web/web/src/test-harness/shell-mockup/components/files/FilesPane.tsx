/**
 * The Files pane: its own nav bar over the tree of one root. The bar and the
 * tree share `labels` and `color`, so one flat set of props feeds both.
 */
import type { Component } from 'solid-js';
import { FileTree, type FileTreeProps } from './FileTree';
import { FilesNavBar, type FilesNavBarProps } from './FilesNavBar';

export type FilesPaneProps = FilesNavBarProps & FileTreeProps;

export const FilesPane: Component<FilesPaneProps> = (props) => (
  <div class="mk-files">
    <FilesNavBar {...props} />
    <FileTree {...props} />
  </div>
);
