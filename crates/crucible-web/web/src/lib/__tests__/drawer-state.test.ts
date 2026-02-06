import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  cycleMode,
  openFlyout,
  closeFlyout,
  pinFlyout,
  addPanel,
  removePanel,
  saveDrawerState,
  loadDrawerState,
  clearDrawerState,
  type DrawerMode,
  type DrawerState,
} from '../drawer-state';

describe('drawer-state', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  describe('cycleMode', () => {
    it('cycles hidden → iconStrip', () => {
      expect(cycleMode('hidden')).toBe('iconStrip');
    });

    it('cycles iconStrip → pinned', () => {
      expect(cycleMode('iconStrip')).toBe('pinned');
    });

    it('cycles pinned → hidden', () => {
      expect(cycleMode('pinned')).toBe('hidden');
    });

    it('completes full cycle', () => {
      let mode: DrawerMode = 'hidden';
      mode = cycleMode(mode);
      expect(mode).toBe('iconStrip');
      mode = cycleMode(mode);
      expect(mode).toBe('pinned');
      mode = cycleMode(mode);
      expect(mode).toBe('hidden');
    });
  });

  describe('openFlyout', () => {
    it('opens flyout in hidden drawer, transitions to iconStrip', () => {
      const state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      const result = openFlyout(state, 'panel-1');

      expect(result.mode).toBe('iconStrip');
      expect(result.activeFlyoutPanel).toBe('panel-1');
      expect(result.panels).toContain('panel-1');
    });

    it('opens flyout in iconStrip drawer, stays in iconStrip', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = openFlyout(state, 'panel-2');

      expect(result.mode).toBe('iconStrip');
      expect(result.activeFlyoutPanel).toBe('panel-2');
      expect(result.panels).toContain('panel-1');
      expect(result.panels).toContain('panel-2');
    });

    it('opens flyout in pinned drawer, stays in pinned', () => {
      const state: DrawerState = {
        mode: 'pinned',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = openFlyout(state, 'panel-2');

      expect(result.mode).toBe('pinned');
      expect(result.activeFlyoutPanel).toBe('panel-2');
    });

    it('does not duplicate panel if already exists', () => {
      const state: DrawerState = {
        mode: 'hidden',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = openFlyout(state, 'panel-1');

      expect(result.panels).toEqual(['panel-1']);
      expect(result.panels.length).toBe(1);
    });

    it('switches active flyout to new panel', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1', 'panel-2'],
        activeFlyoutPanel: 'panel-1',
      };

      const result = openFlyout(state, 'panel-2');

      expect(result.activeFlyoutPanel).toBe('panel-2');
    });
  });

  describe('closeFlyout', () => {
    it('closes flyout, keeps panels', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1', 'panel-2'],
        activeFlyoutPanel: 'panel-1',
      };

      const result = closeFlyout(state);

      expect(result.activeFlyoutPanel).toBeNull();
      expect(result.panels).toEqual(['panel-1', 'panel-2']);
      expect(result.mode).toBe('iconStrip');
    });

    it('closes flyout and collapses to hidden if no panels', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: [],
        activeFlyoutPanel: null,
      };

      const result = closeFlyout(state);

      expect(result.activeFlyoutPanel).toBeNull();
      expect(result.mode).toBe('hidden');
    });

    it('closes flyout in pinned mode, stays pinned', () => {
      const state: DrawerState = {
        mode: 'pinned',
        panels: ['panel-1'],
        activeFlyoutPanel: 'panel-1',
      };

      const result = closeFlyout(state);

      expect(result.activeFlyoutPanel).toBeNull();
      expect(result.mode).toBe('pinned');
    });
  });

  describe('pinFlyout', () => {
    it('pins active flyout, transitions to pinned mode', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: 'panel-1',
      };

      const result = pinFlyout(state);

      expect(result.mode).toBe('pinned');
      expect(result.activeFlyoutPanel).toBeNull();
      expect(result.panels).toContain('panel-1');
    });

    it('does nothing if no active flyout', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = pinFlyout(state);

      expect(result).toEqual(state);
    });

    it('pins flyout with multiple panels', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1', 'panel-2', 'panel-3'],
        activeFlyoutPanel: 'panel-2',
      };

      const result = pinFlyout(state);

      expect(result.mode).toBe('pinned');
      expect(result.panels).toEqual(['panel-1', 'panel-2', 'panel-3']);
      expect(result.activeFlyoutPanel).toBeNull();
    });
  });

  describe('addPanel', () => {
    it('adds panel to empty drawer', () => {
      const state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      const result = addPanel(state, 'panel-1');

      expect(result.panels).toContain('panel-1');
      expect(result.mode).toBe('hidden');
    });

    it('adds panel to existing panels', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = addPanel(state, 'panel-2');

      expect(result.panels).toEqual(['panel-1', 'panel-2']);
    });

    it('does not duplicate panel', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = addPanel(state, 'panel-1');

      expect(result.panels).toEqual(['panel-1']);
    });

    it('does not change mode or flyout', () => {
      const state: DrawerState = {
        mode: 'pinned',
        panels: ['panel-1'],
        activeFlyoutPanel: 'panel-1',
      };

      const result = addPanel(state, 'panel-2');

      expect(result.mode).toBe('pinned');
      expect(result.activeFlyoutPanel).toBe('panel-1');
    });
  });

  describe('removePanel', () => {
    it('removes panel from drawer', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1', 'panel-2'],
        activeFlyoutPanel: null,
      };

      const result = removePanel(state, 'panel-1');

      expect(result.panels).toEqual(['panel-2']);
    });

    it('removes last panel and collapses to hidden', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = removePanel(state, 'panel-1');

      expect(result.panels).toEqual([]);
      expect(result.mode).toBe('hidden');
    });

    it('closes flyout if removed panel was active', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1', 'panel-2'],
        activeFlyoutPanel: 'panel-1',
      };

      const result = removePanel(state, 'panel-1');

      expect(result.activeFlyoutPanel).toBeNull();
      expect(result.panels).toEqual(['panel-2']);
    });

    it('does nothing if panel does not exist', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = removePanel(state, 'panel-2');

      expect(result).toEqual(state);
    });

    it('collapses to hidden when removing last panel in pinned mode', () => {
      const state: DrawerState = {
        mode: 'pinned',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const result = removePanel(state, 'panel-1');

      expect(result.mode).toBe('hidden');
      expect(result.panels).toEqual([]);
    });

    it('keeps mode when removing non-last panel', () => {
      const state: DrawerState = {
        mode: 'pinned',
        panels: ['panel-1', 'panel-2'],
        activeFlyoutPanel: null,
      };

      const result = removePanel(state, 'panel-1');

      expect(result.mode).toBe('pinned');
      expect(result.panels).toEqual(['panel-2']);
    });
  });

  describe('persistence - saveDrawerState / loadDrawerState', () => {
    it('saves and loads drawer state', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1', 'panel-2'],
        activeFlyoutPanel: 'panel-1',
      };

      saveDrawerState('left', state);
      const loaded = loadDrawerState('left');

      expect(loaded).toEqual(state);
    });

    it('saves different states for different zones', () => {
      const leftState: DrawerState = {
        mode: 'iconStrip',
        panels: ['left-panel'],
        activeFlyoutPanel: null,
      };

      const rightState: DrawerState = {
        mode: 'pinned',
        panels: ['right-panel-1', 'right-panel-2'],
        activeFlyoutPanel: 'right-panel-1',
      };

      saveDrawerState('left', leftState);
      saveDrawerState('right', rightState);

      expect(loadDrawerState('left')).toEqual(leftState);
      expect(loadDrawerState('right')).toEqual(rightState);
    });

    it('returns default state if zone not found', () => {
      const loaded = loadDrawerState('left');

      expect(loaded.mode).toBe('hidden');
      expect(loaded.panels).toEqual([]);
      expect(loaded.activeFlyoutPanel).toBeNull();
    });

    it('returns default state if stored data is invalid JSON', () => {
      localStorage.setItem('crucible:drawer:left', 'invalid json');

      const loaded = loadDrawerState('left');

      expect(loaded.mode).toBe('hidden');
      expect(loaded.panels).toEqual([]);
    });

    it('returns default state if mode is invalid', () => {
      const invalid = {
        mode: 'invalid-mode',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      localStorage.setItem('crucible:drawer:left', JSON.stringify(invalid));

      const loaded = loadDrawerState('left');

      expect(loaded.mode).toBe('hidden');
    });

    it('returns default state if panels is not array', () => {
      const invalid = {
        mode: 'iconStrip',
        panels: 'not-an-array',
        activeFlyoutPanel: null,
      };

      localStorage.setItem('crucible:drawer:left', JSON.stringify(invalid));

      const loaded = loadDrawerState('left');

      expect(loaded.mode).toBe('hidden');
    });

    it('returns default state if activeFlyoutPanel is invalid type', () => {
      const invalid = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: 123,
      };

      localStorage.setItem('crucible:drawer:left', JSON.stringify(invalid));

      const loaded = loadDrawerState('left');

      expect(loaded.mode).toBe('hidden');
    });

    it('persists empty panels array', () => {
      const state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      saveDrawerState('left', state);
      const loaded = loadDrawerState('left');

      expect(loaded.panels).toEqual([]);
    });

    it('persists null activeFlyoutPanel', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      saveDrawerState('left', state);
      const loaded = loadDrawerState('left');

      expect(loaded.activeFlyoutPanel).toBeNull();
    });
  });

  describe('clearDrawerState', () => {
    it('removes drawer state from localStorage', () => {
      const state: DrawerState = {
        mode: 'iconStrip',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      saveDrawerState('left', state);
      clearDrawerState('left');

      const loaded = loadDrawerState('left');

      expect(loaded.mode).toBe('hidden');
      expect(loaded.panels).toEqual([]);
    });

    it('does not affect other zones', () => {
      const leftState: DrawerState = {
        mode: 'iconStrip',
        panels: ['left-panel'],
        activeFlyoutPanel: null,
      };

      const rightState: DrawerState = {
        mode: 'pinned',
        panels: ['right-panel'],
        activeFlyoutPanel: null,
      };

      saveDrawerState('left', leftState);
      saveDrawerState('right', rightState);
      clearDrawerState('left');

      expect(loadDrawerState('left').mode).toBe('hidden');
      expect(loadDrawerState('right')).toEqual(rightState);
    });
  });

  describe('edge cases', () => {
    it('handles rapid mode cycling', () => {
      let mode: DrawerMode = 'hidden';

      for (let i = 0; i < 10; i++) {
        mode = cycleMode(mode);
      }

      expect(mode).toBe('iconStrip');
    });

    it('handles multiple panel operations in sequence', () => {
      let state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      state = addPanel(state, 'panel-1');
      state = addPanel(state, 'panel-2');
      state = addPanel(state, 'panel-3');
      state = openFlyout(state, 'panel-2');
      state = removePanel(state, 'panel-1');
      state = removePanel(state, 'panel-3');

      expect(state.panels).toEqual(['panel-2']);
      expect(state.activeFlyoutPanel).toBe('panel-2');
    });

    it('handles opening flyout with invalid panel ID (empty string)', () => {
      const state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      const result = openFlyout(state, '');

      expect(result.activeFlyoutPanel).toBe('');
      expect(result.panels).toContain('');
    });

    it('handles panel IDs with special characters', () => {
      const state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      const panelId = 'panel-with-special-chars-!@#$%';
      const result = addPanel(state, panelId);

      expect(result.panels).toContain(panelId);
    });

    it('handles very long panel ID list', () => {
      let state: DrawerState = {
        mode: 'hidden',
        panels: [],
        activeFlyoutPanel: null,
      };

      for (let i = 0; i < 100; i++) {
        state = addPanel(state, `panel-${i}`);
      }

      expect(state.panels.length).toBe(100);

      state = removePanel(state, 'panel-50');

      expect(state.panels.length).toBe(99);
      expect(state.panels).not.toContain('panel-50');
    });

    it('handles state immutability - original state unchanged', () => {
      const original: DrawerState = {
        mode: 'hidden',
        panels: ['panel-1'],
        activeFlyoutPanel: null,
      };

      const originalPanels = original.panels;

      const result = addPanel(original, 'panel-2');

      expect(original.panels).toEqual(['panel-1']);
      expect(original.panels).toBe(originalPanels);
      expect(result.panels).toEqual(['panel-1', 'panel-2']);
      expect(result.panels).not.toBe(originalPanels);
    });

    it('handles localStorage quota exceeded gracefully', () => {
      const largeState: DrawerState = {
        mode: 'iconStrip',
        panels: Array(1000).fill(null).map((_, i) => `panel-${i}`),
        activeFlyoutPanel: null,
      };

      try {
        saveDrawerState('left', largeState);
      } catch {
        // Expected if quota exceeded
      }

      const loaded = loadDrawerState('left');
      expect(loaded).toBeDefined();
    });
  });
});
