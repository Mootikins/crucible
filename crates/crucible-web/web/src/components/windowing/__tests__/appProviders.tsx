import type { ParentComponent } from 'solid-js';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { WindowingProvider } from '@/windowing/components/context';
import { renderPanel } from '@/lib/render-panel';
import { appWindowSlots } from '@/components/shell/windowSlots';

/** The providers the window manager gives its components, with the app's content and chrome. */
export const AppDragDropProvider: ParentComponent = (props) => (
  <WindowingProvider renderContent={renderPanel} slots={appWindowSlots}>
    <DragDropProvider>{props.children}</DragDropProvider>
  </WindowingProvider>
);
