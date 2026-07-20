import { describe, expect, it } from 'vitest';
import { formatTableLines } from '../table-format';

describe('formatTableLines', () => {
  it('pads cells so pipes align', () => {
    const out = formatTableLines(['| a | long header |', '|---|---|', '| bbbb | c |']);
    expect(out).toEqual([
      '| a    | long header |',
      '| ---- | ----------- |',
      '| bbbb | c           |',
    ]);
  });

  it('preserves alignment colons and pads accordingly', () => {
    const out = formatTableLines(['| L | C | R |', '|:--|:-:|--:|', '| aa | bb | cc |']);
    expect(out).toEqual([
      '| L   |  C  |   R |',
      '| :-- | :-: | --: |',
      '| aa  | bb  |  cc |',
    ]);
  });

  it('keeps escaped pipes inside one cell', () => {
    const out = formatTableLines(['| a\\|b | c |', '|---|---|', '| x | y |']);
    // Two columns — the escaped pipe stays inside the first cell.
    expect(out![0]).toBe('| a\\|b | c   |');
  });

  it('pads ragged rows to the widest row', () => {
    const out = formatTableLines(['| a | b | c |', '|---|---|', '| only |'])!;
    // Every line ends up the same length; short rows gain empty cells.
    const lens = new Set(out.map((l) => l.length));
    expect(lens.size).toBe(1);
    expect(out[2]).toMatch(/^\| only \|\s+\|\s+\|$/);
  });

  it('is idempotent', () => {
    const once = formatTableLines(['| a | b |', '|---|---|', '| ccc | d |'])!;
    expect(formatTableLines(once)).toEqual(once);
  });

  it('returns null for non-table lines', () => {
    expect(formatTableLines(['not a table'])).toBeNull();
    expect(formatTableLines([])).toBeNull();
  });

  it('preserves the first line indent', () => {
    const out = formatTableLines(['  | a | b |', '  |---|---|']);
    expect(out![0].startsWith('  | ')).toBe(true);
    expect(out![1].startsWith('  | ')).toBe(true);
  });
});
