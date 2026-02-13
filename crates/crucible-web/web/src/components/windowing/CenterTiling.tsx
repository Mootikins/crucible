import { Component } from 'solid-js';
import { SplitPane } from './SplitPane';
import { windowStore } from '@/stores/windowStore';

/**
 * Center tiling region: user-configurable binary tree of splits and panes
 * (Dockview-style). Resize via splitter dividers; tabs can be dragged between
 * panes and dropped on edges to create new splits.
 */
export const CenterTiling: Component = () => {
  const layout = () => windowStore.layout;
  return (
    <div class="flex-1 overflow-hidden min-h-0">
      <SplitPane node={layout()} />
    </div>
  );
};
