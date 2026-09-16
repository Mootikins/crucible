import { describe, it, expect } from 'vitest';
import { isEdgeCollapsed, type EdgeMode } from '@/windowing/model/types';

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
