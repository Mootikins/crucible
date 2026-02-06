import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { DrawerState, Zone } from '../drawer-state';
import type { PanelRegistry } from '../panel-registry';
import type { DockviewApi, DockviewPanelApi } from '../promote-dock';
import {
  promoteToCenter,
  dockToDrawer,
  handleCenterDragOver,
  handleCenterDrop,
  CRUCIBLE_PANEL_MIME,
} from '../promote-dock';

function createMockApi(): DockviewApi & {
  _panels: Map<string, DockviewPanelApi>;
} {
  const panels = new Map<string, DockviewPanelApi>();
  return {
    _panels: panels,
    addPanel: vi.fn((opts) => {
      const panel: DockviewPanelApi = { id: opts.id, title: opts.title };
      panels.set(opts.id, panel);
      return panel;
    }),
    removePanel: vi.fn((panel) => {
      panels.delete(panel.id);
    }),
    getPanel: vi.fn((id) => panels.get(id)),
  };
}

function createMockRegistry(...entries: [string, string][]): PanelRegistry {
  const map = new Map(entries.map(([id, title]) => [id, { id, title }]));
  return {
    get: (id: string) => map.get(id),
  } as unknown as PanelRegistry;
}

function makeDrawerState(
  overrides: Partial<DrawerState> = {},
): DrawerState {
  return {
    mode: 'iconStrip',
    panels: [],
    activeFlyoutPanel: null,
    ...overrides,
  };
}

describe('promote-dock', () => {
  let api: ReturnType<typeof createMockApi>;
  let registry: PanelRegistry;

  beforeEach(() => {
    api = createMockApi();
    registry = createMockRegistry(
      ['sessions', 'Sessions'],
      ['files', 'Files'],
      ['editor', 'Editor'],
    );
  });

  describe('promoteToCenter', () => {
    it('removes panel from drawer and calls api.addPanel', () => {
      const state = makeDrawerState({ panels: ['sessions', 'files'] });

      const result = promoteToCenter('sessions', state, api, registry);

      expect(result.panels).toEqual(['files']);
      expect(api.addPanel).toHaveBeenCalledWith({
        id: 'sessions',
        component: 'sessions',
        title: 'Sessions',
      });
      expect(api._panels.has('sessions')).toBe(true);
    });

    it('returns state unchanged when panel not in drawer', () => {
      const state = makeDrawerState({ panels: ['files'] });

      const result = promoteToCenter('nonexistent', state, api, registry);

      expect(result).toBe(state);
      expect(api.addPanel).not.toHaveBeenCalled();
    });

    it('auto-collapses drawer when promoting last panel', () => {
      const state = makeDrawerState({ panels: ['sessions'] });

      const result = promoteToCenter('sessions', state, api, registry);

      expect(result.panels).toEqual([]);
      expect(result.mode).toBe('hidden');
    });

    it('uses panelId as title when not found in registry', () => {
      const emptyRegistry = createMockRegistry();
      const state = makeDrawerState({ panels: ['unknown'] });

      promoteToCenter('unknown', state, api, emptyRegistry);

      expect(api.addPanel).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'unknown' }),
      );
    });
  });

  describe('dockToDrawer', () => {
    it('removes from dockview and adds to drawer', () => {
      api._panels.set('sessions', { id: 'sessions', title: 'Sessions' });
      const state = makeDrawerState({ panels: ['files'] });

      const result = dockToDrawer('sessions', 'left', api, state);

      expect(result.panels).toContain('sessions');
      expect(result.panels).toContain('files');
      expect(api.removePanel).toHaveBeenCalled();
      expect(api._panels.has('sessions')).toBe(false);
    });

    it('auto-shows hidden drawer when receiving a panel', () => {
      api._panels.set('sessions', { id: 'sessions', title: 'Sessions' });
      const state = makeDrawerState({ mode: 'hidden', panels: [] });

      const result = dockToDrawer('sessions', 'left', api, state);

      expect(result.mode).toBe('iconStrip');
      expect(result.panels).toContain('sessions');
    });

    it('returns state unchanged when panel not in dockview', () => {
      const state = makeDrawerState({ panels: ['files'] });

      const result = dockToDrawer('nonexistent', 'left', api, state);

      expect(result).toBe(state);
      expect(api.removePanel).not.toHaveBeenCalled();
    });

    it('preserves existing drawer mode when not hidden', () => {
      api._panels.set('sessions', { id: 'sessions', title: 'Sessions' });
      const state = makeDrawerState({ mode: 'pinned', panels: ['files'] });

      const result = dockToDrawer('sessions', 'left', api, state);

      expect(result.mode).toBe('pinned');
    });
  });

  describe('round-trip', () => {
    it('promote then dock restores original panel list', () => {
      const original = makeDrawerState({ panels: ['sessions', 'files'] });

      const afterPromote = promoteToCenter('sessions', original, api, registry);
      expect(afterPromote.panels).toEqual(['files']);

      const afterDock = dockToDrawer('sessions', 'left', api, afterPromote);
      expect(afterDock.panels).toContain('sessions');
      expect(afterDock.panels).toContain('files');
    });
  });

  describe('handleCenterDragOver', () => {
    it('accepts drag with crucible panel MIME type', () => {
      const accept = vi.fn();
      const event = {
        nativeEvent: {
          dataTransfer: {
            types: [CRUCIBLE_PANEL_MIME],
          },
        } as unknown as DragEvent,
        accept,
      };

      handleCenterDragOver(event);

      expect(accept).toHaveBeenCalledOnce();
    });

    it('does not accept drag without crucible MIME type', () => {
      const accept = vi.fn();
      const event = {
        nativeEvent: {
          dataTransfer: {
            types: ['text/plain'],
          },
        } as unknown as DragEvent,
        accept,
      };

      handleCenterDragOver(event);

      expect(accept).not.toHaveBeenCalled();
    });

    it('handles missing dataTransfer gracefully', () => {
      const accept = vi.fn();
      const event = {
        nativeEvent: {} as DragEvent,
        accept,
      };

      handleCenterDragOver(event);

      expect(accept).not.toHaveBeenCalled();
    });
  });

  describe('handleCenterDrop', () => {
    it('adds panel to dockview and removes from source drawer', () => {
      const leftState = makeDrawerState({ panels: ['sessions', 'files'] });
      const rightState = makeDrawerState({ panels: ['editor'] });
      const bottomState = makeDrawerState({ panels: [] });

      const drawerStates: Record<Zone, DrawerState> = {
        left: leftState,
        right: rightState,
        bottom: bottomState,
      };

      const setDrawerState = vi.fn();

      const event = {
        nativeEvent: {
          dataTransfer: {
            getData: vi.fn().mockReturnValue('sessions'),
          },
        } as unknown as DragEvent,
        group: { id: 'group-1' },
        position: 'center',
      };

      handleCenterDrop(event, api, registry, drawerStates, setDrawerState);

      expect(api.addPanel).toHaveBeenCalledWith({
        id: 'sessions',
        component: 'sessions',
        title: 'Sessions',
        position: { referenceGroup: { id: 'group-1' }, direction: 'center' },
      });

      expect(setDrawerState).toHaveBeenCalledWith('left', expect.objectContaining({
        panels: ['files'],
      }));
    });

    it('does nothing when dataTransfer has no panel ID', () => {
      const drawerStates: Record<Zone, DrawerState> = {
        left: makeDrawerState(),
        right: makeDrawerState(),
        bottom: makeDrawerState(),
      };
      const setDrawerState = vi.fn();

      const event = {
        nativeEvent: {
          dataTransfer: {
            getData: vi.fn().mockReturnValue(''),
          },
        } as unknown as DragEvent,
      };

      handleCenterDrop(event, api, registry, drawerStates, setDrawerState);

      expect(api.addPanel).not.toHaveBeenCalled();
      expect(setDrawerState).not.toHaveBeenCalled();
    });
  });
});
