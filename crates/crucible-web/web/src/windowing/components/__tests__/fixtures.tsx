import type { ParentComponent } from 'solid-js';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { Hexagon, Layers, Package, Target } from '@/lib/icons';
import { configureWindowing } from '@/windowing/store';
import type { WindowPolicy } from '@/windowing/store/policy';
import { WindowingProvider, type WindowingContextValue } from '@/windowing/components/context';
import type { Tab, WindowState } from '@/windowing/model/types';
import { emptyState, generateId } from '@/windowing/model/tree';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';

/**
 * A state with the shape that the component tests need, and no product in it.
 *
 * The centre pane is empty. The left rail is one docked pane with one tab.
 * The right rail is a strip that holds a column: a pane with three tabs over a
 * collapsed pane with one tab. The pane ids are fixed, so a test can name them.
 */
export function railSeed(): WindowState {
  const s = emptyState();
  const centreGroup = generateId();
  const leftGroup = generateId();
  const topGroup = generateId();
  const bottomGroup = generateId();
  const group = (id: string, tabs: Tab[]) => ({ id, tabs, activeTabId: tabs[0]?.id ?? null });
  const centrePane = generateId();
  return {
    ...s,
    layout: { id: centrePane, type: 'pane', tabGroupId: centreGroup },
    tabGroups: {
      [centreGroup]: group(centreGroup, []),
      [leftGroup]: group(leftGroup, [
        { id: 'alpha-tab', title: 'Alpha', contentType: 'alpha', icon: Hexagon },
      ]),
      [topGroup]: group(topGroup, [
        { id: 'beta-tab', title: 'Beta', contentType: 'beta', icon: Layers },
        { id: 'gamma-tab', title: 'Gamma', contentType: 'beta', icon: Package },
        { id: 'delta-tab', title: 'Delta', contentType: 'beta', icon: Layers },
      ]),
      [bottomGroup]: group(bottomGroup, [
        { id: 'omega-tab', title: 'Omega', contentType: 'omega', icon: Target },
      ]),
    },
    edgePanels: {
      left: {
        id: 'left-panel',
        layout: { id: 'left-pane', type: 'pane', tabGroupId: leftGroup },
        mode: 'docked',
        width: 280,
      },
      right: {
        id: 'right-panel',
        layout: {
          id: 'right-split',
          type: 'split',
          direction: 'vertical',
          splitRatio: 0.65,
          first: { id: 'right-pane', type: 'pane', tabGroupId: topGroup },
          second: { id: 'right-term-pane', type: 'pane', tabGroupId: bottomGroup, collapsed: true },
        },
        mode: 'strip',
        width: 340,
      },
    },
    activePaneId: centrePane,
  };
}

/** Configure the core with the neutral policy, seeded by `railSeed`. */
export function configureRails(over: Partial<WindowPolicy> = {}): void {
  configureWindowing(neutralPolicy({ seed: railSeed, ...over }));
}

/** A tab body with no product in it: a box that names the tab. */
export const neutralRenderer: WindowingContextValue['renderContent'] = (tab) => (
  <div data-testid={`content-${tab().contentType}`}>{tab().title}</div>
);

/**
 * The providers that the window manager gives its components: the context
 * and the drag provider. The slots default to none.
 */
export const CoreProviders: ParentComponent<Partial<WindowingContextValue>> = (props) => (
  <WindowingProvider
    renderContent={props.renderContent ?? neutralRenderer}
    slots={props.slots ?? {}}
  >
    <DragDropProvider>{props.children}</DragDropProvider>
  </WindowingProvider>
);
