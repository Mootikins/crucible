import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { EdgeHost } from '../EdgeHost';
import { windowStore, windowActions, setStore } from '@/windowing/store';
import { collectLeafGroupIds, primaryEdgeGroupId } from '@/windowing/model/tree';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { CoreProviders, railSeed, configureRails } from './fixtures';

beforeEach(() => configureRails());

// The old test scraped the rail component and windowing/model/tree.ts for source
// substrings ("{props.tab.icon ? (", "icon: ClipboardList", …). That never
// renders and breaks on renames. Here we render the edge ribbon and assert the
// real output: tabs with an icon render a Lucide <svg>, tabs without one fall
// back to their title initial. The app's own roster test lives with the app
// seed, in stores/__tests__/defaultLayout.icons.test.ts.

beforeEach(() => {
  const fresh = railSeed();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = 'center';
      s.nextZIndex = 100;
    }),
  );
});

// EVERY leaf of the panel's layout, not just the first: a rail is a column,
// and reading only the primary group would silently stop covering the second
// pane.
const edgeTabs = (position: EdgePanelPosition) =>
  collectLeafGroupIds(windowStore.edgePanels[position].layout).flatMap(
    (id) => windowStore.tabGroups[id]?.tabs ?? [],
  );

describe('Ribbon — tab icons', () => {
  it('renders each ribbon tab with its icon as an <svg>', () => {
    const { container } = render(() => (
      <CoreProviders>
        <EdgeHost position="left" />
      </CoreProviders>
    ));

    // The seed gives every rail tab an icon.
    const buttons = container.querySelectorAll('[data-testid="collapsed-tab-button-left"]');
    expect(buttons.length).toBe(edgeTabs('left').length);
    buttons.forEach((btn) => {
      expect(btn.querySelector('svg')).toBeTruthy();
    });
  });

  it('falls back to the title initial for a tab without an icon', () => {
    const groupId = primaryEdgeGroupId(windowStore, 'left')!;
    windowActions.addTab(groupId, { id: 'no-icon-tab', title: 'Zeta', contentType: 'beta' });

    const { container } = render(() => (
      <CoreProviders>
        <EdgeHost position="left" />
      </CoreProviders>
    ));

    const buttons = Array.from(
      container.querySelectorAll<HTMLButtonElement>('[data-testid="collapsed-tab-button-left"]'),
    );
    const fallback = buttons.find((b) => !b.querySelector('svg'));
    expect(fallback, 'a button rendering the title initial instead of an icon').toBeTruthy();
    expect(fallback!.textContent?.trim()).toBe('Z');
  });
});
