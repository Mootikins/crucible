import { describe, it, expect, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { windowStore, windowActions } from '@/windowing/store';
import { findFirstPane } from '@/windowing/model/tree';
import { configureRails, neutralRenderer } from './fixtures';

/**
 * The gate for the split between structure and look.
 *
 * The components of the window manager render structure and state only.
 * `windowing/theme.css` gives each `wm-*` part its look. This test renders the
 * whole window manager in a rich state, walks every element in the DOM, and
 * fails when an element carries a class that sets a look: a colour, a border
 * colour, a radius, a shadow, a ring, an outline, a font size or weight, an
 * opacity, a transition, or a variant (hover, focus, group-hover and so on)
 * of one of these. It reads the rendered DOM, not the source text, so a class
 * that a component builds at run time cannot pass it.
 *
 * Layout utilities stay in the components: display, flex, position, size,
 * overflow, cursor and pointer events. Border WIDTH utilities also stay,
 * because the ribbon border is part of RIBBON_WIDTH_PX.
 */

/** A text utility that sets alignment or wrap, not a colour or a size. */
const TEXT_LAYOUT = /^text-(left|right|center|justify|start|end|wrap|nowrap|balance|pretty|ellipsis|clip)$/;

/** A border utility that sets a width or a style, not a colour. */
const BORDER_LAYOUT = /^border(-[xytrblse])?(-\d+)?$|^border-(solid|dashed|dotted|double|none|hidden)$/;

/** Utility families that only set a look. */
const LOOK_FAMILY =
  /^(bg|outline|ring|shadow|rounded|font|leading|tracking|opacity|fill|stroke|decoration|accent|caret|backdrop|drop-shadow|transition|duration|ease|delay|animate|inset-shadow|inset-ring)(-|$)/;

/** App classes that carry a look: the old tab fade, the focus ring and the mount animations. */
const APP_LOOK = /^(tab-title-fade|focus-ring|cru-anim-)/;

/** CSS properties that set a look, for an arbitrary `[prop:value]` utility. */
const LOOK_PROPERTY =
  /^(color|background|border(-\w+)?-color|border-radius|box-shadow|outline|opacity|font|line-height|mask|-webkit-mask|filter|backdrop-filter|transition|animation)/;

/** The utility without its variants (`hover:`, `data-drop-over:`) and its `!` or `-` prefix. */
function baseOf(token: string): string {
  let depth = 0;
  let start = 0;
  for (let i = 0; i < token.length; i++) {
    const c = token[i];
    if (c === '[' || c === '(') depth++;
    else if (c === ']' || c === ')') depth--;
    else if (c === ':' && depth === 0) start = i + 1;
  }
  return token.slice(start).replace(/^!/, '').replace(/^-/, '');
}

/** True when the class token sets a look. */
function isLookClass(token: string): boolean {
  const base = baseOf(token);
  if (base.startsWith('[')) {
    return LOOK_PROPERTY.test(base.slice(1));
  }
  if (APP_LOOK.test(base)) return true;
  if (base.startsWith('text-')) return !TEXT_LAYOUT.test(base);
  if (base.startsWith('border-')) return !BORDER_LAYOUT.test(base);
  return LOOK_FAMILY.test(base);
}

/** Every class on every element under `root`, with the element's part for the report. */
function lookOffenders(root: Element): string[] {
  const out: string[] = [];
  for (const el of [root, ...root.querySelectorAll('*')]) {
    const cls = el.getAttribute('class') ?? '';
    for (const token of cls.split(/\s+/).filter(Boolean)) {
      if (isLookClass(token)) {
        const part = cls.split(/\s+/).find((t) => t.startsWith('wm-')) ?? el.tagName.toLowerCase();
        out.push(`${part}: ${token}`);
      }
    }
  }
  return out;
}

/** jsdom lays nothing out. The pane markers need a ribbon with a real height. */
function stubRibbonGeometry(container: HTMLElement) {
  const box = (top: number, height: number) =>
    ({ top, bottom: top + height, height, left: 0, right: 40, width: 40, x: 0, y: top }) as DOMRect;
  const ribbon = container.querySelector<HTMLElement>('[data-testid="edge-collapsed-drop-right"]')!;
  ribbon.getBoundingClientRect = () => box(0, 1000);
  const toggle = ribbon.querySelector<HTMLElement>('[data-ribbon-ceiling]');
  if (toggle) toggle.getBoundingClientRect = () => box(0, 36);
  const floor = ribbon.querySelector<HTMLElement>('[data-ribbon-floor]');
  if (floor) floor.getBoundingClientRect = () => box(964, 36);
  const body = container.querySelector<HTMLElement>('[data-edge-panel-body="right"]')!;
  const lower = body.querySelector<HTMLElement>('[data-pane-id="right-term-pane"]');
  if (lower) lower.getBoundingClientRect = () => box(650, 100);
  window.dispatchEvent(new Event('resize'));
}

beforeEach(() => configureRails());

describe('the window manager renders no look of its own', () => {
  it('classifies utilities as look or layout', () => {
    for (const look of [
      'bg-shell-bg',
      'hover:bg-hover-wash',
      'data-drop-over:bg-primary/20',
      'text-muted-dark',
      'text-xs',
      'border-hairline',
      'rounded-md',
      'shadow-lg',
      'ring-1',
      'font-medium',
      'opacity-40',
      'group-hover:opacity-100',
      '-outline-offset-1',
      'transition-colors',
      'focus-ring',
      'cru-anim-pop',
      '[box-shadow:0_0_0_1px_red]',
    ]) {
      expect(isLookClass(look), look).toBe(true);
    }
    for (const layout of [
      'flex',
      'w-10',
      'h-9',
      'border-r',
      'border',
      'text-left',
      'truncate',
      'cursor-col-resize',
      'active:cursor-grabbing',
      "after:content-['']",
      'after:-inset-x-1',
      'max-w-(--cru-measure-tab)',
      '[scrollbar-width:none]',
      'wm-tab',
    ]) {
      expect(isLookClass(layout), layout).toBe(false);
    }
  });

  it('puts every look in the theme: no element carries a look class', async () => {
    // A rich state: both rails open, a column of two open panes in the right
    // rail, a split centre with tabs and an empty pane, a modified tab, a
    // floating window and a rolled-up one.
    windowActions.setEdgePanelCollapsed('right', false);
    windowActions.setPaneCollapsed('right-term-pane', false);
    const centre = findFirstPane(windowStore.layout)!;
    const centreGroup = centre.tabGroupId!;
    windowActions.addTab(centreGroup, { id: 'tab-one', title: 'One', contentType: 'alpha' });
    windowActions.addTab(centreGroup, { id: 'tab-two', title: 'Two', contentType: 'alpha' });
    windowActions.updateTab(centreGroup, 'tab-two', { isModified: true });
    windowActions.splitPane(centre.id, 'vertical');
    const floatGroup = windowActions.createTabGroup();
    windowActions.addTab(floatGroup, { id: 'tab-float', title: 'Float', contentType: 'alpha' });
    windowActions.createFloatingWindow(floatGroup, 100, 100, 400, 300);
    const rolledGroup = windowActions.createTabGroup();
    windowActions.addTab(rolledGroup, { id: 'tab-rolled', title: 'Rolled', contentType: 'alpha' });
    windowActions.createFloatingWindow(rolledGroup, 200, 200, 300, 200);
    const rolled = windowStore.floatingWindows.find((w) => w.tabGroupId === rolledGroup)!;
    windowActions.minimizeFloatingWindow(rolled.id);

    const { container } = render(() => (
      <WindowManager
        renderContent={neutralRenderer}
        slots={{ emptyPaneHints: () => [{ label: 'Open a file', chord: 'Ctrl+P' }] }}
      />
    ));
    stubRibbonGeometry(container);
    await waitFor(() =>
      expect(container.querySelector('[data-testid="ribbon-pane-marker-right"]')).not.toBeNull(),
    );

    // The walk must see every kind of part, or it proves nothing.
    for (const part of [
      'wm-root',
      'wm-edge-host',
      'wm-edge-body',
      'wm-edge-handle',
      'wm-ribbon',
      'wm-ribbon-btn',
      'wm-ribbon-toggle',
      'wm-ribbon-tab',
      'wm-ribbon-leading',
      'wm-ribbon-trailing',
      'wm-ribbon-cmd',
      'wm-ribbon-tail',
      'wm-pane-marker',
      'wm-pane-boundary',
      'wm-pane',
      'wm-drop-zone',
      'wm-splitter',
      'wm-tabbar',
      'wm-tabstrip',
      'wm-tab',
      'wm-tab-title',
      'wm-tab-grip',
      'wm-tab-dot',
      'wm-tab-close',
      'wm-tabbar-btn',
      'wm-empty-pane',
      'wm-empty-card',
      'wm-empty-kbd',
      'wm-floating',
      'wm-floating-titlebar',
      'wm-floating-btn',
      'wm-floating-body',
      'wm-minimized-bar',
      'wm-minimized-btn',
    ]) {
      expect(container.querySelector(`.${part}`), part).not.toBeNull();
    }

    expect(lookOffenders(container)).toEqual([]);
  });
});
