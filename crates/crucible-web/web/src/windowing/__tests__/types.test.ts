import { describe, it, expect, expectTypeOf } from 'vitest';
import {
  EDGE_CUES,
  EDGE_MODES,
  EXPAND_EXITS,
  isEdgeCollapsed,
  type EdgeCue,
  type EdgeMode,
  type ExpandExit,
} from '@/windowing/model/types';

describe('the closed edge tables', () => {
  it('name every member of their union', () => {
    // `satisfies` refuses a member that is not in the union. These two lines
    // refuse a union member that is not in the table; tsc checks them.
    expectTypeOf<Exclude<EdgeMode, (typeof EDGE_MODES)[number]>>().toEqualTypeOf<never>();
    expectTypeOf<Exclude<EdgeCue, (typeof EDGE_CUES)[number]>>().toEqualTypeOf<never>();
    expectTypeOf<Exclude<ExpandExit, (typeof EXPAND_EXITS)[number]>>().toEqualTypeOf<never>();
    expect(new Set(EDGE_MODES).size).toBe(EDGE_MODES.length);
    expect(new Set(EDGE_CUES).size).toBe(EDGE_CUES.length);
    expect(new Set(EXPAND_EXITS).size).toBe(EXPAND_EXITS.length);
  });
});

describe('isEdgeCollapsed', () => {
  it.each<[EdgeMode, boolean]>([
    ['docked', false],
    ['strip', true],
    ['flyout', true],
    ['hidden', true],
  ])('returns %s -> %s', (mode, collapsed) => {
    expect(isEdgeCollapsed({ mode })).toBe(collapsed);
  });
});
