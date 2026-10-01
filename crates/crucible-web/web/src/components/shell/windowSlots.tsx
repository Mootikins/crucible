import { openPanelTab } from '@/lib/panel-actions';
import { treeRootActions } from '@/stores/treeRootStore';
import type { WindowingSlots } from '@/windowing';
import { railHead, railTail } from './RailChrome';
import { CornerBar } from './CornerBar';
import { attachPaneDropTarget } from '@/lib/file-dnd';
import { emptyPaneHints } from '@/lib/keyboard-shortcuts';

/** The chrome that the app hangs on the window manager. */
export const appWindowSlots: WindowingSlots = {
  tabMenuActions: tab => {
    const path = tab.metadata?.filePath;
    return typeof path === 'string' ? [{
      id: 'show-in-file-tree', label: 'Show in file tree',
      run: () => { treeRootActions.reveal(path); openPanelTab('files'); },
    }] : [];
  },
  railHead,
  railTail,
  corner: () => <CornerBar />,
  emptyPaneHints,
  attachDropTarget: attachPaneDropTarget,
};
