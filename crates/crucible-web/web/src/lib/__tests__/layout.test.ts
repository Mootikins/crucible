// src/lib/__tests__/layout.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  loadZoneState,
  migrateZoneState,
  DEFAULT_ZONE_STATE,
  loadZoneWidths,
  DEFAULT_ZONE_WIDTHS,
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
});
