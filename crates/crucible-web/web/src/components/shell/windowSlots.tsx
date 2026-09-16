import type { WindowingSlots } from '@/windowing/components/context';
import { railHead, railTail } from './RailChrome';
import { CornerBar } from './CornerBar';
import { attachPaneDropTarget } from '@/lib/file-dnd';
import { emptyPaneHints } from '@/lib/keyboard-shortcuts';

/** The chrome that the app hangs on the window manager. */
export const appWindowSlots: WindowingSlots = {
  railHead,
  railTail,
  corner: () => <CornerBar />,
  emptyPaneHints,
  attachDropTarget: attachPaneDropTarget,
};
