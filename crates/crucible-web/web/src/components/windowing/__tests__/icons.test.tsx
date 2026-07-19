import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import type { Component } from 'solid-js';
import * as icons from '../icons';

// icons.tsx re-exports Lucide-Solid components under Icon* aliases. The old
// test scraped the source for the absence of "<svg" and hard-coded a stale
// count ("13 icons" while the array actually listed 14, and the module has
// since grown to more). This version imports the real module and RENDERS each
// export, proving they are genuine SVG-emitting components — which is the
// property the source scrape was a weak proxy for.

// The canonical roster (kept in sync with icons.tsx). Every entry must exist
// AND render an <svg>; any extra exports are also validated by the second test.
const EXPECTED_ICONS = [
  'IconPanelLeft',
  'IconPanelLeftClose',
  'IconPanelRight',
  'IconPanelRightClose',
  'IconPanelBottom',
  'IconPanelBottomClose',
  'IconSettings',
  'IconZap',
  'IconLayout',
  'IconGripVertical',
  'IconGripHorizontal',
  'IconClose',
  'IconMaximize',
  'IconMinimize',
  'IconPin',
  'IconTabBar',
  'IconBell',
] as const;

describe('windowing/icons.tsx', () => {
  it('exports every named icon as a component that renders an <svg>', () => {
    for (const name of EXPECTED_ICONS) {
      const Icon = icons[name as keyof typeof icons] as Component<{ class?: string }>;
      expect(typeof Icon).toBe('function');

      const { container, unmount } = render(() => <Icon class="w-4 h-4" />);
      const svg = container.querySelector('svg');
      expect(svg, `${name} should render an <svg>`).toBeTruthy();
      // Lucide passes the class through to the rendered <svg>.
      expect(svg?.getAttribute('class') ?? '').toContain('w-4');
      unmount();
    }
  });

  it('every exported member is a renderable icon component (no stray non-components)', () => {
    const entries = Object.entries(icons);
    // Guard against silent drift: the module must not shrink below the roster.
    expect(entries.length).toBeGreaterThanOrEqual(EXPECTED_ICONS.length);

    for (const [name, value] of entries) {
      expect(typeof value, `${name} should be a component function`).toBe('function');
      const Icon = value as Component<{ class?: string }>;
      const { container, unmount } = render(() => <Icon class="w-4 h-4" />);
      expect(container.querySelector('svg'), `${name} should render an <svg>`).toBeTruthy();
      unmount();
    }
  });
});
