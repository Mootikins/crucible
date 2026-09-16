import { describe, it, expect, vi } from 'vitest';
import type { Component } from 'solid-js';
import { serializeLayout, deserializeLayout } from '@/windowing/model/serializer';
import type {
  LayoutCodecHooks,
  RestoredLayout,
  SerializedLayout,
  SerializedLayoutV9,
} from '@/windowing/model/serializer';
import type { EdgeMode, WindowState } from '@/windowing/model/types';

type Kind = 'alpha' | 'beta';

const Icon: Component<{ class?: string }> = () => null;

/** No tab gets an icon. */
const noIcon = (): undefined => undefined;

/** Hooks that do nothing, so each test sees only what the core does. */
function stubHooks(over: Partial<LayoutCodecHooks<Kind>> = {}): LayoutCodecHooks<Kind> {
  return {
    upgradeLegacy: () => {
      throw new Error('upgradeLegacy must not run');
    },
    prune: () => {},
    ...over,
  };
}

/** A minimal state with two rails, built here so the test needs no app seed. */
function defaultLayoutFixture(): WindowState<Kind> {
  const rail = (pos: 'left' | 'right') => ({
    id: `${pos}-panel`,
    layout: { id: `${pos}-pane`, type: 'pane' as const, tabGroupId: `${pos}-group` },
    mode: 'docked' as EdgeMode,
    width: 280,
  });
  return {
    layout: { id: 'centre', type: 'pane', tabGroupId: 'centre-group' },
    tabGroups: {
      'centre-group': {
        id: 'centre-group',
        tabs: [{ id: 't1', title: 'One', contentType: 'alpha', icon: Icon }],
        activeTabId: 't1',
      },
      'left-group': { id: 'left-group', tabs: [], activeTabId: null },
      'right-group': { id: 'right-group', tabs: [], activeTabId: null },
    },
    edgePanels: { left: rail('left'), right: rail('right') },
    floatingWindows: [],
  } as unknown as WindowState<Kind>;
}

/** A stored v9 layout: the rails carry `isCollapsed`, not `mode`. */
function v9Fixture(): SerializedLayoutV9<Kind> {
  return {
    version: 9,
    layout: { id: 'p', type: 'pane', tabGroupId: 'g' },
    tabGroups: {
      g: { id: 'g', tabs: [{ id: 't', title: 'T', contentType: 'beta' }], activeTabId: 't' },
    },
    edgePanels: {
      left: { id: 'l', layout: { id: 'lp', type: 'pane', tabGroupId: null }, isCollapsed: true, width: 280 },
      right: { id: 'r', layout: { id: 'rp', type: 'pane', tabGroupId: null }, isCollapsed: false, width: 340 },
    },
    floatingWindows: [],
  };
}

describe('v9 → v10: edge modes', () => {
  it('maps isCollapsed to strip, and its absence to docked', () => {
    const v9 = v9Fixture();
    delete v9.edgePanels.right.isCollapsed;
    const out = deserializeLayout(v9, stubHooks(), noIcon);
    expect(out.edgePanels.left.mode).toBe('strip');
    expect(out.edgePanels.right.mode).toBe('docked');
  });

  it('maps isCollapsed false to docked', () => {
    const out = deserializeLayout(v9Fixture(), stubHooks(), noIcon);
    expect(out.edgePanels.right.mode).toBe('docked');
  });

  it('round-trips a mode', () => {
    const state = defaultLayoutFixture();
    state.edgePanels.left.mode = 'hidden';
    state.edgePanels.left.cue = 'none';
    const json = serializeLayout(state);
    expect(json.version).toBe(10);
    const back = deserializeLayout(json, stubHooks(), noIcon);
    expect(back.edgePanels.left.mode).toBe('hidden');
    expect(back.edgePanels.left.cue).toBe('none');
  });

  it.each<EdgeMode>(['docked', 'strip', 'flyout', 'hidden'])(
    'keeps the %s mode and the none cue through a save and a reload',
    (mode) => {
      const state = defaultLayoutFixture();
      state.edgePanels.left.mode = mode;
      state.edgePanels.left.cue = 'none';
      const json = JSON.parse(JSON.stringify(serializeLayout(state))) as SerializedLayout<Kind>;
      expect(json.edgePanels.left).not.toHaveProperty('isCollapsed');
      const back = deserializeLayout(json, stubHooks(), noIcon);
      expect(back.edgePanels.left.mode).toBe(mode);
      expect(back.edgePanels.left.cue).toBe('none');
    },
  );
});

describe('the core reader and its hooks', () => {
  it('asks the app to upgrade a payload older than v9, and only then', () => {
    const upgradeLegacy = vi.fn(() => v9Fixture());
    const hooks = stubHooks({ upgradeLegacy });
    const out = deserializeLayout({ version: 3 }, hooks, noIcon);
    expect(upgradeLegacy).toHaveBeenCalledOnce();
    expect(out.edgePanels.left.mode).toBe('strip');

    upgradeLegacy.mockClear();
    deserializeLayout(v9Fixture(), hooks, noIcon);
    deserializeLayout(serializeLayout(defaultLayoutFixture()), hooks, noIcon);
    expect(upgradeLegacy).not.toHaveBeenCalled();
  });

  it.each([0, 1.5, 11, Number.NaN])('refuses version %s', (version) => {
    expect(() => deserializeLayout({ version }, stubHooks(), noIcon)).toThrow(/Unsupported layout version/);
  });

  it('refuses an upgrade that does not reach v9', () => {
    const hooks = stubHooks({ upgradeLegacy: () => ({ ...v9Fixture(), version: 8 as 9 }) });
    expect(() => deserializeLayout({ version: 2 }, hooks, noIcon)).toThrow(
      new Error('Legacy layout upgrade from v2 returned v8, not v9'),
    );
  });

  it('reads an unknown mode as docked', () => {
    const json = serializeLayout(defaultLayoutFixture()) as unknown as {
      edgePanels: Record<string, Record<string, unknown>>;
    };
    json.edgePanels.left!.mode = 'floating';
    const out = deserializeLayout(json as unknown as SerializedLayout<Kind>, stubHooks(), noIcon);
    expect(out.edgePanels.left.mode).toBe('docked');
  });

  it('drops an unknown cue, and keeps a known one', () => {
    const state = defaultLayoutFixture();
    state.edgePanels.right.cue = 'grip';
    const json = serializeLayout(state) as unknown as {
      edgePanels: Record<string, Record<string, unknown>>;
    };
    json.edgePanels.left!.cue = 'glow';
    const out = deserializeLayout(json as unknown as SerializedLayout<Kind>, stubHooks(), noIcon);
    expect(out.edgePanels.left).not.toHaveProperty('cue');
    expect(out.edgePanels.right.cue).toBe('grip');
  });

  it('rebuilds an absent rail before the app prunes', () => {
    const v9 = v9Fixture() as unknown as { edgePanels: Record<string, unknown> };
    delete v9.edgePanels.right;
    let seen: RestoredLayout<Kind> | undefined;
    deserializeLayout(v9 as unknown as SerializedLayoutV9<Kind>, stubHooks({
      prune: (state) => { seen = structuredClone(state); },
    }), noIcon);
    expect(seen?.edgePanels.right).toEqual({
      id: 'right-panel',
      layout: { id: 'right-pane', type: 'pane', tabGroupId: null },
      mode: 'strip',
      width: 280,
    });
  });

  it('prunes, and then gives each surviving tab its icon', () => {
    const order: string[] = [];
    const out = deserializeLayout(serializeLayout(defaultLayoutFixture()), stubHooks({
      prune: (state) => {
        order.push('prune');
        expect(state.tabGroups['centre-group']!.tabs[0]!.icon).toBeUndefined();
        state.tabGroups['centre-group']!.tabs.push({ id: 't2', title: 'Two', contentType: 'beta' });
      },
    }), (kind) => {
      order.push(`icon:${kind}`);
      return kind === 'beta' ? Icon : undefined;
    });
    expect(order).toEqual(['prune', 'icon:alpha', 'icon:beta']);
    const tabs = out.tabGroups['centre-group']!.tabs;
    expect(tabs.map((t) => t.icon)).toEqual([undefined, Icon]);
  });

  it('does not write the icon into the saved layout', () => {
    const json = serializeLayout(defaultLayoutFixture());
    expect(json.tabGroups['centre-group']!.tabs[0]).not.toHaveProperty('icon');
  });
});
