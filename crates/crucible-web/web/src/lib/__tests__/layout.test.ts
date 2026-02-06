// src/lib/__tests__/layout.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  loadZoneState,
  saveZoneState,
  isValidZoneState,
  migrateZoneState,
  DEFAULT_ZONE_STATE,
  loadZoneWidths,
  saveZoneWidths,
  DEFAULT_ZONE_WIDTHS,
  saveZoneLayout,
  loadZoneLayout,
  migrateOldLayout,
  type ZoneState,
  type ZoneWidths,
} from '../layout';

describe('layout - ZoneMode migration', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  describe('ZoneMode type', () => {
    it('accepts visible state', () => {
      const state: ZoneState = { left: 'visible', right: 'visible', bottom: 'hidden' };
      expect(state.left).toBe('visible');
    });

    it('accepts hidden state', () => {
      const state: ZoneState = { left: 'hidden', right: 'visible', bottom: 'hidden' };
      expect(state.left).toBe('hidden');
    });

    it('accepts pinned state', () => {
      const state: ZoneState = { left: 'pinned', right: 'visible', bottom: 'hidden' };
      expect(state.left).toBe('pinned');
    });
  });

  describe('migrateZoneState', () => {
    it('converts boolean true to visible', () => {
      const oldState = { left: true, right: true, bottom: false };
      const migrated = migrateZoneState(oldState);
      expect(migrated).toEqual({
        left: 'visible',
        right: 'visible',
        bottom: 'hidden',
      });
    });

    it('converts boolean false to hidden', () => {
      const oldState = { left: false, right: true, bottom: false };
      const migrated = migrateZoneState(oldState);
      expect(migrated).toEqual({
        left: 'hidden',
        right: 'visible',
        bottom: 'hidden',
      });
    });

    it('passes through ZoneMode strings unchanged', () => {
      const newState = { left: 'visible', right: 'pinned', bottom: 'hidden' };
      const migrated = migrateZoneState(newState);
      expect(migrated).toEqual(newState);
    });

    it('returns DEFAULT_ZONE_STATE for null input', () => {
      const migrated = migrateZoneState(null);
      expect(migrated).toEqual(DEFAULT_ZONE_STATE);
    });

    it('returns DEFAULT_ZONE_STATE for undefined input', () => {
      const migrated = migrateZoneState(undefined);
      expect(migrated).toEqual(DEFAULT_ZONE_STATE);
    });

    it('returns DEFAULT_ZONE_STATE for garbage input', () => {
      const migrated = migrateZoneState('garbage');
      expect(migrated).toEqual(DEFAULT_ZONE_STATE);
    });

    it('returns DEFAULT_ZONE_STATE for invalid object', () => {
      const migrated = migrateZoneState({ left: 'invalid' });
      expect(migrated).toEqual(DEFAULT_ZONE_STATE);
    });

    it('returns DEFAULT_ZONE_STATE for missing properties', () => {
      const migrated = migrateZoneState({ left: true });
      expect(migrated).toEqual(DEFAULT_ZONE_STATE);
    });
  });

  describe('isValidZoneState', () => {
    it('accepts new ZoneMode format', () => {
      const state = { left: 'visible', right: 'pinned', bottom: 'hidden' };
      expect(isValidZoneState(state)).toBe(true);
    });

    it('accepts old boolean format for backward compatibility', () => {
      const state = { left: true, right: false, bottom: true };
      expect(isValidZoneState(state)).toBe(true);
    });

    it('rejects mixed boolean and ZoneMode', () => {
      const state = { left: true, right: 'visible', bottom: false };
      expect(isValidZoneState(state)).toBe(false);
    });

    it('rejects invalid ZoneMode strings', () => {
      const state = { left: 'invalid', right: 'visible', bottom: 'hidden' };
      expect(isValidZoneState(state)).toBe(false);
    });

    it('rejects null', () => {
      expect(isValidZoneState(null)).toBe(false);
    });

    it('rejects undefined', () => {
      expect(isValidZoneState(undefined)).toBe(false);
    });

    it('rejects missing properties', () => {
      const state = { left: 'visible', right: 'visible' };
      expect(isValidZoneState(state)).toBe(false);
    });
  });

  describe('loadZoneState', () => {
    it('returns DEFAULT_ZONE_STATE when localStorage is empty', () => {
      const state = loadZoneState();
      expect(state).toEqual(DEFAULT_ZONE_STATE);
    });

    it('loads and migrates old boolean format from localStorage', () => {
      localStorage.setItem('crucible:zones', JSON.stringify({ left: true, right: true, bottom: false }));
      const state = loadZoneState();
      expect(state).toEqual({
        left: 'visible',
        right: 'visible',
        bottom: 'hidden',
      });
    });

    it('loads new ZoneMode format from localStorage unchanged', () => {
      const stored = { left: 'visible', right: 'pinned', bottom: 'hidden' };
      localStorage.setItem('crucible:zones', JSON.stringify(stored));
      const state = loadZoneState();
      expect(state).toEqual(stored);
    });

    it('returns DEFAULT_ZONE_STATE on invalid JSON', () => {
      localStorage.setItem('crucible:zones', 'not valid json');
      const state = loadZoneState();
      expect(state).toEqual(DEFAULT_ZONE_STATE);
    });

    it('returns DEFAULT_ZONE_STATE on corrupted data', () => {
      localStorage.setItem('crucible:zones', JSON.stringify({ left: 'invalid', right: 'invalid', bottom: 'invalid' }));
      const state = loadZoneState();
      expect(state).toEqual(DEFAULT_ZONE_STATE);
    });
  });

  describe('saveZoneState', () => {
    it('saves ZoneMode format to localStorage', () => {
      const state: ZoneState = { left: 'visible', right: 'pinned', bottom: 'hidden' };
      saveZoneState(state);
      const stored = JSON.parse(localStorage.getItem('crucible:zones')!);
      expect(stored).toEqual(state);
    });

    it('persists across multiple saves', () => {
      const state1: ZoneState = { left: 'visible', right: 'visible', bottom: 'hidden' };
      saveZoneState(state1);
      
      const state2: ZoneState = { left: 'pinned', right: 'hidden', bottom: 'visible' };
      saveZoneState(state2);
      
      const stored = JSON.parse(localStorage.getItem('crucible:zones')!);
      expect(stored).toEqual(state2);
    });
  });

  describe('DEFAULT_ZONE_STATE', () => {
    it('has correct default values', () => {
      expect(DEFAULT_ZONE_STATE).toEqual({
        left: 'visible',
        right: 'visible',
        bottom: 'hidden',
      });
    });

    it('uses ZoneMode strings not booleans', () => {
      expect(typeof DEFAULT_ZONE_STATE.left).toBe('string');
      expect(typeof DEFAULT_ZONE_STATE.right).toBe('string');
      expect(typeof DEFAULT_ZONE_STATE.bottom).toBe('string');
    });
  });
});

describe('layout - Zone widths', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  describe('DEFAULT_ZONE_WIDTHS', () => {
    it('has correct default values', () => {
      expect(DEFAULT_ZONE_WIDTHS).toEqual({
        left: 280,
        right: 350,
        bottom: 200,
      });
    });
  });

  describe('loadZoneWidths', () => {
    it('returns defaults when localStorage is empty', () => {
      const widths = loadZoneWidths();
      expect(widths).toEqual(DEFAULT_ZONE_WIDTHS);
    });

    it('loads saved widths from localStorage', () => {
      const saved: ZoneWidths = { left: 300, right: 400, bottom: 250 };
      localStorage.setItem('crucible:zone-widths', JSON.stringify(saved));
      const widths = loadZoneWidths();
      expect(widths).toEqual(saved);
    });

    it('returns defaults on invalid JSON', () => {
      localStorage.setItem('crucible:zone-widths', 'not json');
      const widths = loadZoneWidths();
      expect(widths).toEqual(DEFAULT_ZONE_WIDTHS);
    });

    it('returns defaults when values are not numbers', () => {
      localStorage.setItem('crucible:zone-widths', JSON.stringify({ left: 'wide', right: 350, bottom: 200 }));
      const widths = loadZoneWidths();
      expect(widths).toEqual(DEFAULT_ZONE_WIDTHS);
    });

    it('returns defaults when values are zero or negative', () => {
      localStorage.setItem('crucible:zone-widths', JSON.stringify({ left: 0, right: -10, bottom: 200 }));
      const widths = loadZoneWidths();
      expect(widths).toEqual(DEFAULT_ZONE_WIDTHS);
    });

    it('returns defaults when properties are missing', () => {
      localStorage.setItem('crucible:zone-widths', JSON.stringify({ left: 280 }));
      const widths = loadZoneWidths();
      expect(widths).toEqual(DEFAULT_ZONE_WIDTHS);
    });
  });

  describe('saveZoneWidths', () => {
    it('saves widths to localStorage', () => {
      const widths: ZoneWidths = { left: 300, right: 400, bottom: 250 };
      saveZoneWidths(widths);
      const stored = JSON.parse(localStorage.getItem('crucible:zone-widths')!);
      expect(stored).toEqual(widths);
    });

    it('overwrites previous values', () => {
      saveZoneWidths({ left: 100, right: 100, bottom: 100 });
      saveZoneWidths({ left: 300, right: 400, bottom: 250 });
      const stored = JSON.parse(localStorage.getItem('crucible:zone-widths')!);
      expect(stored).toEqual({ left: 300, right: 400, bottom: 250 });
    });

    it('round-trips through loadZoneWidths', () => {
      const widths: ZoneWidths = { left: 320, right: 380, bottom: 180 };
      saveZoneWidths(widths);
      const loaded = loadZoneWidths();
      expect(loaded).toEqual(widths);
    });
  });
});

describe('layout - Per-zone serialization', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  const mockLayout = {
    grid: { root: { type: 'branch', size: 100 } },
    panels: { panel1: { id: 'panel1', title: 'Test' } },
  };

  describe('saveZoneLayout', () => {
    it('saves zone layout to zone-specific key', () => {
      const serialized = JSON.stringify(mockLayout);
      saveZoneLayout('left', serialized);
      const stored = localStorage.getItem('crucible:layout:left');
      expect(stored).toBe(serialized);
    });

    it('saves different zones to different keys', () => {
      const leftLayout = JSON.stringify({ ...mockLayout, zone: 'left' });
      const centerLayout = JSON.stringify({ ...mockLayout, zone: 'center' });
      
      saveZoneLayout('left', leftLayout);
      saveZoneLayout('center', centerLayout);
      
      expect(localStorage.getItem('crucible:layout:left')).toBe(leftLayout);
      expect(localStorage.getItem('crucible:layout:center')).toBe(centerLayout);
    });

    it('overwrites previous layout for same zone', () => {
      const layout1 = JSON.stringify({ ...mockLayout, version: 1 });
      const layout2 = JSON.stringify({ ...mockLayout, version: 2 });
      
      saveZoneLayout('right', layout1);
      saveZoneLayout('right', layout2);
      
      expect(localStorage.getItem('crucible:layout:right')).toBe(layout2);
    });
  });

  describe('loadZoneLayout', () => {
    it('loads zone layout from zone-specific key', () => {
      const serialized = JSON.stringify(mockLayout);
      localStorage.setItem('crucible:layout:left', serialized);
      
      const loaded = loadZoneLayout('left');
      expect(loaded).toBe(serialized);
    });

    it('returns null when zone layout does not exist', () => {
      const loaded = loadZoneLayout('center');
      expect(loaded).toBeNull();
    });

    it('returns null on invalid JSON', () => {
      localStorage.setItem('crucible:layout:bottom', 'not valid json');
      const loaded = loadZoneLayout('bottom');
      expect(loaded).toBeNull();
    });

    it('returns null when layout missing grid property', () => {
      const invalid = JSON.stringify({ panels: { panel1: {} } });
      localStorage.setItem('crucible:layout:left', invalid);
      const loaded = loadZoneLayout('left');
      expect(loaded).toBeNull();
    });

    it('returns null when layout missing panels property', () => {
      const invalid = JSON.stringify({ grid: { root: {} } });
      localStorage.setItem('crucible:layout:left', invalid);
      const loaded = loadZoneLayout('left');
      expect(loaded).toBeNull();
    });

    it('validates each zone independently', () => {
      const valid = JSON.stringify(mockLayout);
      const invalid = JSON.stringify({ grid: {} });
      
      localStorage.setItem('crucible:layout:left', valid);
      localStorage.setItem('crucible:layout:center', invalid);
      
      expect(loadZoneLayout('left')).toBe(valid);
      expect(loadZoneLayout('center')).toBeNull();
    });
  });

  describe('migrateOldLayout', () => {
    it('migrates old layout to center zone', () => {
      const oldLayout = JSON.stringify(mockLayout);
      localStorage.setItem('crucible:layout', oldLayout);
      
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout:center')).toBe(oldLayout);
      expect(localStorage.getItem('crucible:layout')).toBeNull();
    });

    it('does nothing when old layout does not exist', () => {
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout')).toBeNull();
      expect(localStorage.getItem('crucible:layout:center')).toBeNull();
    });

    it('clears old key after successful migration', () => {
      const oldLayout = JSON.stringify(mockLayout);
      localStorage.setItem('crucible:layout', oldLayout);
      
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout')).toBeNull();
    });

    it('clears old key on parse error', () => {
      localStorage.setItem('crucible:layout', 'invalid json');
      
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout')).toBeNull();
      expect(localStorage.getItem('crucible:layout:center')).toBeNull();
    });

    it('clears old key when layout missing grid property', () => {
      const invalid = JSON.stringify({ panels: {} });
      localStorage.setItem('crucible:layout', invalid);
      
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout')).toBeNull();
    });

    it('clears old key when layout missing panels property', () => {
      const invalid = JSON.stringify({ grid: {} });
      localStorage.setItem('crucible:layout', invalid);
      
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout')).toBeNull();
    });

    it('does not affect existing per-zone layouts', () => {
      const oldLayout = JSON.stringify(mockLayout);
      const existingLeft = JSON.stringify({ ...mockLayout, zone: 'left' });
      
      localStorage.setItem('crucible:layout', oldLayout);
      localStorage.setItem('crucible:layout:left', existingLeft);
      
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout:left')).toBe(existingLeft);
      expect(localStorage.getItem('crucible:layout:center')).toBe(oldLayout);
    });

    it('can be called multiple times safely', () => {
      const oldLayout = JSON.stringify(mockLayout);
      localStorage.setItem('crucible:layout', oldLayout);
      
      migrateOldLayout();
      migrateOldLayout();
      
      expect(localStorage.getItem('crucible:layout')).toBeNull();
      expect(localStorage.getItem('crucible:layout:center')).toBe(oldLayout);
    });
  });
});
