import { describe, it, expect, beforeEach } from 'vitest';
import { Show } from 'solid-js';
import { fireEvent, render } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { useFloatingWindow, type FloatingWindowHandle } from '../WindowControls';
import { windowStore, windowActions } from '@/windowing/store';
import { collectLeafGroupIds, findFirstPane } from '@/windowing/model/tree';
import type { WindowingContextValue } from '@/windowing/components/context';
import type { FloatingWindow } from '@/windowing/model/types';
import { configureRails, neutralRenderer } from './fixtures';

/**
 * `floatingChrome` decides where the controls of a floating window sit. With
 * `titlebar` (the default) the window draws a title bar. With `merged` the
 * controls sit in the tab bar of the window, or, for a window without a tab
 * bar, in the content through `useFloatingWindow()`.
 */

const mount = (renderContent: WindowingContextValue['renderContent'] = neutralRenderer) =>
  render(() => <WindowManager renderContent={renderContent} slots={{}} />);

/** A floating window with two tabs of the `float` type. Returns its id. */
function floatTwoTabs(opts: Pick<FloatingWindow, 'transient' | 'showTabBar'> = {}): string {
  const group = windowActions.createTabGroup();
  windowActions.addTab(group, { id: 'float-one', title: 'One', contentType: 'float' });
  windowActions.addTab(group, { id: 'float-two', title: 'Two', contentType: 'float' });
  return windowActions.createFloatingWindow(group, 100, 100, 400, 300, opts);
}

const win = (id: string) => windowStore.floatingWindows.find((w) => w.id === id);
const floating = (c: HTMLElement) => c.querySelector<HTMLElement>('.wm-floating')!;
const control = (c: HTMLElement, testId: string) =>
  floating(c).querySelector<HTMLElement>(`[data-testid="${testId}"]`)!;

/** Press at (x, y), move by (dx, dy), release. */
function drag(el: Element, dx: number, dy: number) {
  fireEvent.mouseDown(el, { clientX: 150, clientY: 110, button: 0 });
  fireEvent.mouseMove(document, { clientX: 150 + dx, clientY: 110 + dy });
  fireEvent.mouseUp(document, { clientX: 150 + dx, clientY: 110 + dy });
}

beforeEach(() => configureRails());

describe('the floating chrome setting', () => {
  it('defaults to the title bar, which holds the controls', () => {
    floatTwoTabs();
    const { container } = mount();
    expect(windowStore.floatingChrome).toBe('titlebar');
    const titlebar = floating(container).querySelector('.wm-floating-titlebar');
    expect(titlebar).not.toBeNull();
    expect(titlebar!.querySelector('[data-testid="float-maximize"]')).not.toBeNull();
    expect(floating(container).querySelector('.wm-window-controls')).toBeNull();
  });

  it('merged: no title bar, and the controls sit in the tab bar actions of the window', () => {
    windowActions.setFloatingChrome('merged');
    floatTwoTabs();
    const { container } = mount();
    expect(floating(container).querySelector('.wm-floating-titlebar')).toBeNull();
    const controls = floating(container).querySelector('.wm-tabbar-actions .wm-window-controls');
    expect(controls).not.toBeNull();
    // One set of controls, not one per region.
    expect(container.querySelectorAll('.wm-window-controls')).toHaveLength(1);
  });

  it('merged: docked tab bars get no controls', () => {
    windowActions.setFloatingChrome('merged');
    const centre = findFirstPane(windowStore.layout)!.tabGroupId!;
    windowActions.addTab(centre, { id: 'docked', title: 'Docked', contentType: 'alpha' });
    const { container } = mount();
    expect(container.querySelector('.wm-window-controls')).toBeNull();
    expect(container.querySelector('.wm-tabbar[data-wm-drag-handle]')).toBeNull();
  });

  it('survives a layout reset: it is the user\'s setting, not the layout\'s', () => {
    windowActions.setFloatingChrome('merged');
    windowActions.resetLayoutToDefaults();
    expect(windowStore.floatingChrome).toBe('merged');
  });

  it('survives a restore, and the saved layout does not hold it', () => {
    windowActions.setFloatingChrome('merged');
    const saved = windowActions.exportLayout();
    expect(JSON.stringify(saved)).not.toContain('floatingChrome');
    windowActions.importLayout(saved);
    expect(windowStore.floatingChrome).toBe('merged');
  });
});

describe('each merged control still acts on the window', () => {
  beforeEach(() => windowActions.setFloatingChrome('merged'));

  it('minimize rolls the window up', () => {
    const id = floatTwoTabs();
    const { container } = mount();
    fireEvent.click(control(container, 'float-minimize'));
    expect(win(id)!.isMinimized).toBe(true);
  });

  it('maximize, then restore', () => {
    const id = floatTwoTabs();
    const { container } = mount();
    fireEvent.click(control(container, 'float-maximize'));
    expect(win(id)!.isMaximized).toBe(true);
    fireEvent.click(control(container, 'float-maximize'));
    expect(win(id)!.isMaximized).toBe(false);
  });

  it('close closes the window', () => {
    const id = floatTwoTabs();
    const { container } = mount();
    fireEvent.click(control(container, 'float-close'));
    expect(win(id)).toBeUndefined();
  });

  it('dock moves the tabs into the centre', () => {
    const id = floatTwoTabs();
    const { container } = mount();
    fireEvent.click(control(container, 'float-dock'));
    expect(win(id)).toBeUndefined();
    const centreTabs = collectLeafGroupIds(windowStore.layout).flatMap(
      (g) => windowStore.tabGroups[g]?.tabs.map((t) => t.id) ?? [],
    );
    expect(centreTabs).toEqual(expect.arrayContaining(['float-one', 'float-two']));
  });

  it('pin promotes a transient window, and the pin goes away', () => {
    const id = floatTwoTabs({ transient: true });
    const { container } = mount();
    fireEvent.click(control(container, 'float-pin'));
    expect(win(id)!.transient).toBeFalsy();
    expect(floating(container).querySelector('[data-testid="float-pin"]')).toBeNull();
  });

  it('the tab bar toggle hides the bar, and the controls leave with it', () => {
    const id = floatTwoTabs();
    const { container } = mount();
    fireEvent.click(control(container, 'float-tabbar-toggle'));
    expect(win(id)!.showTabBar).toBe(false);
    expect(floating(container).querySelector('.wm-tabbar')).toBeNull();
  });
});

describe('useFloatingWindow', () => {
  it('is null for docked content, and gives the controls inside a floating window', () => {
    const seen: Record<string, FloatingWindowHandle | null> = {};
    const probe: WindowingContextValue['renderContent'] = (tab) => {
      seen[tab().contentType] = useFloatingWindow();
      return <div />;
    };
    const centre = findFirstPane(windowStore.layout)!.tabGroupId!;
    windowActions.addTab(centre, { id: 'docked', title: 'Docked', contentType: 'docked' });
    const id = floatTwoTabs();
    mount(probe);
    expect(seen.docked).toBeNull();
    const fw = seen.float;
    expect(fw).not.toBeNull();
    expect(fw!.id).toBe(id);
    expect(fw!.chrome()).toBe('titlebar');
    expect(fw!.hasTabBar()).toBe(true);
    expect(typeof fw!.controls).toBe('function');
    windowActions.setFloatingChrome('merged');
    windowActions.updateFloatingWindow(id, { showTabBar: false });
    expect(fw!.chrome()).toBe('merged');
    expect(fw!.hasTabBar()).toBe(false);
  });
});

/**
 * The app's side of a tabless window: a nav bar that hosts the controls. The
 * docked tabs of the seed keep the neutral body.
 */
const navRenderer: WindowingContextValue['renderContent'] = (tab) => {
  if (tab().contentType !== 'float') return neutralRenderer(tab);
  const fw = useFloatingWindow();
  return (
    <div>
      <nav data-testid="doc-nav" data-wm-drag-handle="">
        <span>{tab().title}</span>
        <button type="button" data-testid="doc-back">Back</button>
        <Show when={fw && fw.chrome() === 'merged' && !fw.hasTabBar()}>
          {fw && <fw.controls />}
        </Show>
      </nav>
      <p data-testid="doc-text">Body</p>
    </div>
  );
};

describe('a tabless merged window', () => {
  beforeEach(() => windowActions.setFloatingChrome('merged'));

  it('gives its controls to the content, and draws no title bar', () => {
    floatTwoTabs({ showTabBar: false });
    const { container, getByTestId } = mount(navRenderer);
    expect(getByTestId('doc-nav').querySelector('.wm-window-controls')).not.toBeNull();
    expect(floating(container).querySelector('.wm-floating-titlebar')).toBeNull();
    expect(container.querySelectorAll('.wm-window-controls')).toHaveLength(1);
  });

  it('the content controls act on the window', () => {
    const id = floatTwoTabs({ showTabBar: false });
    const { getByTestId } = mount(navRenderer);
    fireEvent.click(getByTestId('doc-nav').querySelector('[data-testid="float-tabbar-toggle"]')!);
    expect(win(id)!.showTabBar).toBe(true);
    // The tab bar is back, so the controls move into it.
    expect(getByTestId('doc-nav').querySelector('.wm-window-controls')).toBeNull();
    expect(document.querySelector('.wm-floating .wm-tabbar-actions .wm-window-controls')).not.toBeNull();
  });

  it('draws its title bar when the content does not take the controls', () => {
    floatTwoTabs({ showTabBar: false });
    const { container } = mount();
    expect(floating(container).querySelector('.wm-floating-titlebar')).not.toBeNull();
  });
});

describe('the drag handles', () => {
  it('merged: a press on the empty part of the tab bar moves the window', () => {
    windowActions.setFloatingChrome('merged');
    const id = floatTwoTabs();
    const { container } = mount();
    drag(floating(container).querySelector('.wm-tabstrip')!, 40, 25);
    expect(win(id)).toMatchObject({ x: 140, y: 125 });
  });

  it('merged: a press on a tab or on a control does not move the window', () => {
    windowActions.setFloatingChrome('merged');
    const id = floatTwoTabs();
    const { container } = mount();
    drag(floating(container).querySelector('[data-tab-id="float-one"]')!, 40, 25);
    drag(floating(container).querySelector('.wm-tab-title')!, 40, 25);
    drag(control(container, 'float-dock'), 40, 25);
    expect(win(id)).toMatchObject({ x: 100, y: 100 });
  });

  it('titlebar: the tab bar is not a drag handle, and the title bar is', () => {
    const id = floatTwoTabs();
    const { container } = mount();
    drag(floating(container).querySelector('.wm-tabstrip')!, 40, 25);
    expect(win(id)).toMatchObject({ x: 100, y: 100 });
    drag(floating(container).querySelector('.wm-floating-title')!, 40, 25);
    expect(win(id)).toMatchObject({ x: 140, y: 125 });
  });

  it('a tabless window: a data-wm-drag-handle element moves it, a button inside it does not', () => {
    windowActions.setFloatingChrome('merged');
    const id = floatTwoTabs({ showTabBar: false });
    const { getByTestId } = mount(navRenderer);
    drag(getByTestId('doc-back'), 40, 25);
    drag(getByTestId('doc-text'), 40, 25);
    expect(win(id)).toMatchObject({ x: 100, y: 100 });
    drag(getByTestId('doc-nav'), 40, 25);
    expect(win(id)).toMatchObject({ x: 140, y: 125 });
  });

  it('a drag does not move a maximized window', () => {
    windowActions.setFloatingChrome('merged');
    const id = floatTwoTabs();
    windowActions.maximizeFloatingWindow(id);
    const { container } = mount();
    const before = { x: win(id)!.x, y: win(id)!.y };
    drag(floating(container).querySelector('.wm-tabstrip')!, 40, 25);
    expect(win(id)).toMatchObject(before);
  });

  it('a drag pins a transient window', () => {
    windowActions.setFloatingChrome('merged');
    const id = floatTwoTabs({ transient: true });
    const { container } = mount();
    drag(floating(container).querySelector('.wm-tabstrip')!, 40, 25);
    expect(win(id)!.transient).toBeFalsy();
  });
});
