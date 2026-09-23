import { describe, it, expect } from 'vitest';
import { dragSpan } from '../diff-comments';

// A row is a line number, or `n - 0.5` for a removed row of the chunk above
// line `n`.
describe('dragSpan', () => {
  it('takes the lines between the two ends, in either direction', () => {
    expect(dragSpan(3, 5)).toEqual({ first: 3, last: 5 });
    expect(dragSpan(8, 3)).toEqual({ first: 3, last: 8 });
    expect(dragSpan(4, 4)).toEqual({ first: 4, last: 4 });
  });

  it('a removed row at the bottom end takes the line under its chunk', () => {
    expect(dragSpan(3, 4.5)).toEqual({ first: 3, last: 5 });
  });

  it('a removed row at the top end takes the line above its chunk', () => {
    expect(dragSpan(8, 4.5)).toEqual({ first: 4, last: 8 });
  });

  it('a drag inside one removed chunk takes the lines on both sides of it', () => {
    expect(dragSpan(4.5, 4.5)).toEqual({ first: 4, last: 5 });
  });

  it('a chunk above line 1 has no line above it', () => {
    expect(dragSpan(0.5, 2)).toEqual({ first: 1, last: 2 });
  });
});
