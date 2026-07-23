import { describe, it, expect } from 'vitest';
import { kilnLabel } from '@/lib/kiln-label';

describe('kilnLabel', () => {
  it('prefers the registered name', () => {
    expect(kilnLabel('/home/u/.crucible', 'crucible-docs')).toBe('crucible-docs');
  });
  it('falls back to the basename', () => {
    expect(kilnLabel('/home/u/kilns/notes')).toBe('notes');
  });
  it('labels any .crucible data dir (and empty path) as Home kiln', () => {
    expect(kilnLabel('/home/u/.crucible')).toBe('Home kiln');
    expect(kilnLabel('/home/u/.crucible/')).toBe('Home kiln');
    expect(kilnLabel('')).toBe('Home kiln');
    expect(kilnLabel('/x', '   ')).toBe('x');
  });
});
