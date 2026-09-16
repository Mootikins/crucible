import { expect, type Page } from '@playwright/test';
import { disableAnimations } from '../helpers/geometry';

/**
 * Helpers for the core windowing specs.
 *
 * The core specs open `windowing-harness.html`, which mounts the window
 * manager with the neutral policy and no app: no panel registry, no rails
 * rule, no server. Nothing here may import an app fixture or mock the API.
 *
 * The neutral seed (src/windowing/testing/neutralPolicy.ts):
 *  - centre: `tab-alpha` (active) and `tab-beta`
 *  - left rail, docked: `tab-left`
 *  - right rail, strip: `tab-right`
 */

export type Region = 'center' | 'left' | 'right';

export interface LayoutNode {
  type: 'pane' | 'split';
  id: string;
  tabGroupId?: string | null;
  direction?: 'horizontal' | 'vertical';
  splitRatio?: number;
  collapsed?: boolean;
  first?: LayoutNode;
  second?: LayoutNode;
}

export interface StoreSnapshot {
  layout: LayoutNode;
  tabGroups: Record<
    string,
    { id: string; tabs: Array<{ id: string; title: string; contentType: string }>; activeTabId: string | null }
  >;
  edgePanels: Record<'left' | 'right', { id: string; layout: LayoutNode; mode: string; width?: number }>;
  floatingWindows: Array<{ id: string; tabGroupId: string; x: number; y: number; width: number; height: number }>;
  activePaneId: string | null;
}

/** Open the harness and wait for the seed to paint. */
export async function openHarness(page: Page): Promise<void> {
  await disableAnimations(page);
  await page.goto('/windowing-harness.html');
  await expect(page.locator('[data-tab-id="tab-alpha"]')).toBeVisible();
}

/** A plain copy of the store. Icons are functions, so the copy drops them. */
export function readStore(page: Page): Promise<StoreSnapshot> {
  return page.evaluate(
    () => JSON.parse(JSON.stringify((window as unknown as { __windowStore: unknown }).__windowStore)) as never,
  );
}

/** Call one window action in the page, and return its result. */
export function act(page: Page, action: string, ...args: unknown[]): Promise<unknown> {
  return page.evaluate(
    ([name, params]) => {
      const actions = (window as unknown as { __windowActions: Record<string, (...a: unknown[]) => unknown> })
        .__windowActions;
      const fn = actions[name as string];
      if (!fn) throw new Error(`no window action named ${name as string}`);
      return fn(...(params as unknown[]));
    },
    [action, args] as const,
  );
}

/** Every pane of a layout tree, first to last. */
export function panesOf(node: LayoutNode): LayoutNode[] {
  return node.type === 'pane' ? [node] : [...panesOf(node.first!), ...panesOf(node.second!)];
}

/** The depth of a layout tree. A single pane has depth 0. */
export function depthOf(node: LayoutNode): number {
  return node.type === 'pane' ? 0 : 1 + Math.max(depthOf(node.first!), depthOf(node.second!));
}

/** The layout tree of a region. */
function layoutOf(s: StoreSnapshot, region: Region): LayoutNode {
  return region === 'center' ? s.layout : s.edgePanels[region].layout;
}

/** The tab group ids of a region, first to last. */
export async function groupIds(page: Page, region: Region): Promise<string[]> {
  const s = await readStore(page);
  return panesOf(layoutOf(s, region))
    .map((p) => p.tabGroupId)
    .filter((id): id is string => !!id && !!s.tabGroups[id]);
}

/** The tab ids of a region, in strip order. */
export async function tabIds(page: Page, region: Region): Promise<string[]> {
  const s = await readStore(page);
  return panesOf(layoutOf(s, region)).flatMap((p) =>
    (p.tabGroupId ? s.tabGroups[p.tabGroupId]?.tabs ?? [] : []).map((t) => t.id),
  );
}

/**
 * Give every empty centre pane a tab.
 *
 * An empty pane yields its width, and the splitter beside it goes inert. A
 * spec that drags a splitter needs content on both sides.
 */
export async function fillCentrePanes(page: Page): Promise<void> {
  const s = await readStore(page);
  for (const pane of panesOf(s.layout)) {
    const id = pane.tabGroupId;
    if (!id || !s.tabGroups[id] || s.tabGroups[id]!.tabs.length > 0) continue;
    await act(page, 'addTab', id, { id: `filler-${id}`, title: 'Filler', contentType: 'gamma' });
  }
}

/**
 * A pointer drag: down, move in steps, up.
 *
 * The window manager uses pointer events, not HTML5 drag and drop, so a
 * spec drives the mouse.
 */
export async function pointerDrag(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
  steps = 10,
): Promise<void> {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps });
  await page.mouse.up();
}

/** A centre tab, not the rail copy of a tab with the same id. */
export const centreTab = (page: Page, id: string) =>
  page.locator(`[data-tab-id="${id}"]:not([data-testid^="edge-tab-"])`);
