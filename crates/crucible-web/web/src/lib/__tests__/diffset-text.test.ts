import { describe, it, expect } from 'vitest';
import { quickfixLine, quickfixList, referenceForm, type DiffComment } from '../diffset';

// The text forms of a comment. They mirror `reference` and `quickfix_line` in
// `crucible-core/src/diff.rs`, so the web pane and `cru diff comments` agree.

const PATH = 'crates/a/src/lib.rs';

function comment(start: number, end: number, body: string, resolved = false): DiffComment {
  return {
    id: `c-${start}`,
    diffset: 'session-chat-1',
    anchor: { kind: 'commit', id: 'abc' },
    author: 'human',
    body,
    created_at: '2026-09-21T10:00:00Z',
    line_range: { start, end },
    root: '/repo',
    path: PATH,
    quoted: '',
    resolved,
    side: 'current',
  };
}

/** The Vim default `errorformat` `%f:%l:%m` needs this shape. */
const ERRORFORMAT = /^[^:]+:\d+:.*$/;

describe('the text forms of a comment', () => {
  it('one line comment matches errorformat', () => {
    const line = quickfixLine(comment(626, 627, 'needs a test'));
    expect(line).toBe(`${PATH}:626: needs a test`);
    expect(line).toMatch(ERRORFORMAT);
  });

  it('range comment matches errorformat', () => {
    const line = quickfixLine(comment(626, 629, 'this deny path needs a test'));
    expect(line).toBe(`${PATH}:626: [626-628] this deny path needs a test`);
    expect(line).toMatch(ERRORFORMAT);
  });

  it('a second line is indented', () => {
    const lines = quickfixLine(comment(3, 5, 'first\nsecond\n\nfourth\n')).split('\n');
    expect(lines).toEqual([`${PATH}:3: [3-4] first`, '  second', '  ', '  fourth']);
    expect(lines[0]).toMatch(ERRORFORMAT);
  });

  it('the reference form is inclusive', () => {
    expect(referenceForm(PATH, 626, 629)).toBe(`${PATH}:626-628`);
    expect(referenceForm(PATH, 12, 13)).toBe(`${PATH}:12`);
  });

  it('the list leaves out a resolved comment', () => {
    const list = quickfixList([comment(1, 2, 'open'), comment(5, 6, 'done', true)]);
    expect(list).toBe(`${PATH}:1: open\n`);
  });
});
