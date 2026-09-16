import { createContext, useContext, type JSX, type ParentComponent } from 'solid-js';
import type { EdgePanelPosition, Tab } from '../model/types';

/** What the app hangs on the window manager's chrome. Every slot is optional. */
export interface WindowingSlots {
  /** Above the tab icons on a rail. */
  railHead?: (position: EdgePanelPosition) => JSX.Element;
  /** Pinned to the far end of a rail. */
  railTail?: (position: EdgePanelPosition) => JSX.Element;
  /** The floating cluster at the bottom-right of the centre. */
  corner?: () => JSX.Element;
  /** Rows that an empty pane prints under its label. */
  emptyPaneHints?: () => { label: string; chord: string }[];
  /** Attach a native drop target to a pane body. Returns the cleanup. */
  attachDropTarget?: (el: HTMLElement, groupId: () => string | null) => () => void;
}

export interface WindowingContextValue {
  /** The body of a tab. The core knows no content type, so the app draws it. */
  renderContent: (tab: Tab) => JSX.Element;
  slots: WindowingSlots;
}

const Ctx = createContext<WindowingContextValue>();

export const WindowingProvider: ParentComponent<WindowingContextValue> = (props) => (
  // Getters, so that a descendant reads the current prop and not a copy.
  <Ctx.Provider
    value={{
      get renderContent() {
        return props.renderContent;
      },
      get slots() {
        return props.slots;
      },
    }}
  >
    {props.children}
  </Ctx.Provider>
);

/** The content and chrome of the nearest provider. Throws without one. */
export function useWindowing(): WindowingContextValue {
  const v = useContext(Ctx);
  if (!v) throw new Error('windowing: useWindowing needs a WindowingProvider');
  return v;
}
