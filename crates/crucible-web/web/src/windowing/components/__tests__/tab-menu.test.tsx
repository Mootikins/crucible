import { describe, it, expect, beforeEach } from 'vitest';
import { fireEvent, render, waitFor } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { windowStore, windowActions } from '@/windowing/store';
import { collectLeafGroupIds, findFirstPane } from '@/windowing/model/tree';
import { configureRails, neutralRenderer } from './fixtures';

/**
 * The right-click menu of a tab moves the tab between the layout and a
 * floating window, as the panel menu of an Adobe app does. A docked tab shows
 * "Pop out". A floating tab shows "Dock". The icon of a rail tab in the ribbon
 * shows the same menu, because a theme can hide the tab bars of a rail.
 */

const mount = () => render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);

/** Every tab renders its own menu, closed. Only the open one counts. */
const openMenu = () => document.querySelector<HTMLElement>('[role="menu"][data-state="open"]');
const menuItems = () =>
  Array.from(openMenu()?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
const menuLabels = () => menuItems().map((i) => i.textContent?.trim());

/** zag highlights on pointerdown and selects the HIGHLIGHTED item on click. */
const chooseMenuItem = (label: string) => {
  const item = menuItems().find((i) => i.textContent?.trim() === label)!;
  fireEvent.pointerDown(item);
  fireEvent.click(item);
};

/** The group of the one floating window that holds `tabId`, or undefined. */
const floatingGroupOf = (tabId: string) =>
  windowStore.floatingWindows
    .map((w) => windowStore.tabGroups[w.tabGroupId])
    .find((g) => g?.tabs.some((t) => t.id === tabId));

/** The tabs that the centre tiling shows. */
const centreTabIds = () =>
  collectLeafGroupIds(windowStore.layout).flatMap(
    (id) => windowStore.tabGroups[id]?.tabs.map((t) => t.id) ?? [],
  );

let centreGroup: string;

beforeEach(() => {
  configureRails();
  centreGroup = findFirstPane(windowStore.layout)!.tabGroupId!;
  windowActions.addTab(centreGroup, { id: 'tab-one', title: 'One', contentType: 'alpha' });
  windowActions.addTab(centreGroup, { id: 'tab-two', title: 'Two', contentType: 'alpha' });
});

describe('the tab menu of a docked tab', () => {
  it('offers Pop out and no Dock', async () => {
    const { container } = mount();
    fireEvent.contextMenu(container.querySelector('[data-tab-id="tab-one"]')!);
    await waitFor(() => expect(menuLabels()).toContain('Pop out'));
    expect(menuLabels()).not.toContain('Dock');
  });

  it('Pop out moves the tab into a floating window, and the pane keeps the rest', async () => {
    const { container } = mount();
    fireEvent.contextMenu(container.querySelector('[data-tab-id="tab-one"]')!);
    await waitFor(() => expect(menuLabels()).toContain('Pop out'));
    chooseMenuItem('Pop out');

    await waitFor(() => expect(floatingGroupOf('tab-one')).toBeDefined());
    expect(floatingGroupOf('tab-one')!.tabs.map((t) => t.id)).toEqual(['tab-one']);
    expect(centreTabIds()).toEqual(['tab-two']);
  });

  describe('with a policy that keeps tab-one', () => {
    beforeEach(() => {
      configureRails({ mayCloseTab: (_s, _g, tabId) => tabId !== 'tab-one' });
      centreGroup = findFirstPane(windowStore.layout)!.tabGroupId!;
      windowActions.addTab(centreGroup, { id: 'tab-one', title: 'One', contentType: 'alpha' });
      windowActions.addTab(centreGroup, { id: 'tab-two', title: 'Two', contentType: 'alpha' });
    });

    it('offers no Pop out on the kept tab', async () => {
      const { container } = mount();
      fireEvent.contextMenu(container.querySelector('[data-tab-id="tab-one"]')!);
      await waitFor(() => expect(menuLabels()).toContain('Close Others'));
      expect(menuLabels()).not.toContain('Pop out');
    });

    it('still offers Pop out on the other tab', async () => {
      const { container } = mount();
      fireEvent.contextMenu(container.querySelector('[data-tab-id="tab-two"]')!);
      await waitFor(() => expect(menuLabels()).toContain('Pop out'));
    });
  });

  it('the keyboard reaches Pop out and chooses it', async () => {
    const { container } = mount();
    fireEvent.contextMenu(container.querySelector('[data-tab-id="tab-one"]')!);
    await waitFor(() => expect(menuLabels()).toContain('Pop out'));

    const content = openMenu()!;
    const highlighted = () =>
      content.querySelector('[role="menuitem"][data-highlighted]')?.textContent?.trim();
    // The arrow keys move the highlight down the rows until it reaches the row.
    for (let i = 0; i < menuItems().length && highlighted() !== 'Pop out'; i++) {
      fireEvent.keyDown(content, { key: 'ArrowDown' });
      // The menu machine applies each key on a later task.
      await new Promise((r) => setTimeout(r, 0));
    }
    expect(highlighted()).toBe('Pop out');
    fireEvent.keyDown(content, { key: 'Enter' });

    await waitFor(() => expect(floatingGroupOf('tab-one')).toBeDefined());
  });
});

describe('the tab menu of a floating tab', () => {
  beforeEach(() => {
    windowActions.popOutPane(findFirstPane(windowStore.layout)!.id, 'tab-one');
  });

  it('offers Dock and no Pop out', async () => {
    const { container } = mount();
    const floatingTab = container.querySelector('.wm-floating [data-tab-id="tab-one"]')!;
    fireEvent.contextMenu(floatingTab);
    await waitFor(() => expect(menuLabels()).toContain('Dock'));
    expect(menuLabels()).not.toContain('Pop out');
  });

  it('Dock moves the tab back into a docked pane', async () => {
    const { container } = mount();
    fireEvent.contextMenu(container.querySelector('.wm-floating [data-tab-id="tab-one"]')!);
    await waitFor(() => expect(menuLabels()).toContain('Dock'));
    chooseMenuItem('Dock');

    await waitFor(() => expect(centreTabIds()).toContain('tab-one'));
    expect(floatingGroupOf('tab-one')).toBeUndefined();
    expect(windowStore.floatingWindows).toHaveLength(0);
  });
});

describe('the tab menu of a ribbon icon', () => {
  const ribbonTab = (container: HTMLElement, title: string) =>
    Array.from(
      container.querySelectorAll<HTMLElement>('[data-testid="collapsed-tab-button-right"]'),
    ).find((b) => b.getAttribute('title') === title)!;

  it('Pop out moves the rail tab into a floating window', async () => {
    const { container } = mount();
    fireEvent.contextMenu(ribbonTab(container, 'Gamma'));
    await waitFor(() => expect(menuLabels()).toContain('Pop out'));
    chooseMenuItem('Pop out');

    await waitFor(() => expect(floatingGroupOf('gamma-tab')).toBeDefined());
    const railTabs = collectLeafGroupIds(windowStore.edgePanels.right.layout).flatMap(
      (id) => windowStore.tabGroups[id]?.tabs.map((t) => t.id) ?? [],
    );
    expect(railTabs).not.toContain('gamma-tab');
    expect(railTabs).toContain('beta-tab');
  });

  it('offers no Pop out on a tab that the policy calls unavailable', async () => {
    configureRails({ unavailableReason: (t) => (t.id === 'gamma-tab' ? 'not here' : null) });
    const { container } = mount();
    fireEvent.contextMenu(ribbonTab(container, 'Gamma — not here'));
    await waitFor(() => expect(menuLabels()).toContain('Close Others'));
    expect(menuLabels()).not.toContain('Pop out');
  });
});
