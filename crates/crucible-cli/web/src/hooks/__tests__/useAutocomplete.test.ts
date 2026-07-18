import { describe, it, expect } from 'vitest';
import { fuzzyFilter, type AutocompleteItem } from '@/hooks/useAutocomplete';

const item = (label: string): AutocompleteItem => ({ id: label, label, insertText: label });

describe('useAutocomplete fuzzyFilter', () => {
  it('ranks by relevance, not raw daemon order', () => {
    // Daemon order puts the weaker match first; fuzzy ranking must reorder.
    const items = [item('Meeting Notes'), item('Notes'), item('Nonsense')];
    const out = fuzzyFilter(items, 'notes').map((i) => i.label);
    // Exact/tighter match ranks above the looser one; non-matches drop.
    expect(out[0]).toBe('Notes');
    expect(out).toContain('Meeting Notes');
    expect(out).not.toContain('Nonsense');
  });

  it('returns all items unchanged for an empty query', () => {
    const items = [item('b'), item('a')];
    expect(fuzzyFilter(items, '').map((i) => i.label)).toEqual(['b', 'a']);
  });
});
