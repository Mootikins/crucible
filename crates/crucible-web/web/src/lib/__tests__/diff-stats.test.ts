import { describe, it, expect } from 'vitest';
import { analyzeDiff } from '../diff-stats';

describe('analyzeDiff', () => {
  it('treats a pure substitution as one addition and one deletion', () => {
    const result = analyzeDiff('a', 'b');
    expect(result.additions).toBe(1);
    expect(result.deletions).toBe(1);
    expect(result.lines).toHaveLength(2);
    // diff order is delete-then-add for a substitution
    expect(result.lines[0].type).toBe('remove');
    expect(result.lines[0].content).toBe('a');
    expect(result.lines[1].type).toBe('add');
    expect(result.lines[1].content).toBe('b');
  });

  it('treats identical content as a single context line with no changes', () => {
    const result = analyzeDiff('a', 'a');
    expect(result.additions).toBe(0);
    expect(result.deletions).toBe(0);
    expect(result.lines).toHaveLength(1);
    expect(result.lines[0].type).toBe('context');
    expect(result.lines[0].content).toBe('a');
    expect(result.lines[0].oldLineNum).toBe(1);
    expect(result.lines[0].newLineNum).toBe(1);
  });

  it('returns no lines when both sides are empty', () => {
    const result = analyzeDiff('', '');
    expect(result.additions).toBe(0);
    expect(result.deletions).toBe(0);
    expect(result.lines).toHaveLength(0);
  });

  it('treats a one-sided empty diff as pure additions', () => {
    const result = analyzeDiff('', 'a\nb\n');
    expect(result.additions).toBe(2);
    expect(result.deletions).toBe(0);
    // All resulting lines must be additions
    expect(result.lines.every((l) => l.type === 'add')).toBe(true);
    expect(result.lines.map((l) => l.content)).toEqual(['a', 'b']);
  });

  it('treats removing all content as pure deletions', () => {
    const result = analyzeDiff('a\nb\n', '');
    expect(result.additions).toBe(0);
    expect(result.deletions).toBe(2);
    expect(result.lines.every((l) => l.type === 'remove')).toBe(true);
    expect(result.lines.map((l) => l.content)).toEqual(['a', 'b']);
  });

  it('populates oldLineNum/newLineNum correctly for a mixed diff', () => {
    // ctx / remove / add / ctx
    const result = analyzeDiff('keep1\nold\nkeep2', 'keep1\nnew\nkeep2');

    // Find the lines by type+content so we don't depend on subtle ordering
    // beyond what the contract guarantees (context is interleaved with
    // remove-then-add for substitutions).
    const byKey = (type: string, content: string) =>
      result.lines.find((l) => l.type === type && l.content === content);

    const keep1 = byKey('context', 'keep1');
    expect(keep1).toBeDefined();
    expect(keep1!.oldLineNum).toBe(1);
    expect(keep1!.newLineNum).toBe(1);

    const removed = byKey('remove', 'old');
    expect(removed).toBeDefined();
    expect(removed!.oldLineNum).toBe(2);
    expect(removed!.newLineNum).toBeNull();

    const added = byKey('add', 'new');
    expect(added).toBeDefined();
    expect(added!.oldLineNum).toBeNull();
    expect(added!.newLineNum).toBe(2);

    const keep2 = byKey('context', 'keep2');
    expect(keep2).toBeDefined();
    expect(keep2!.oldLineNum).toBe(3);
    expect(keep2!.newLineNum).toBe(3);

    expect(result.additions).toBe(1);
    expect(result.deletions).toBe(1);
  });

  it('stats match the per-line counts (no drift between paths)', () => {
    // Property: additions/deletions on the analysis must equal a manual
    // recount of lines[].type — this is what the old DiffViewer .filter()
    // path computed independently, and it has to stay in sync.
    const cases: Array<[string, string]> = [
      ['a', 'b'],
      ['a\n', 'a\nb\nc\n'],
      ['a\nb\nc\n', 'a\n'],
      ['', 'new line'],
      ['same', 'same'],
      ['', ''],
    ];
    for (const [oldC, newC] of cases) {
      const a = analyzeDiff(oldC, newC);
      const addsByLine = a.lines.filter((l) => l.type === 'add').length;
      const delsByLine = a.lines.filter((l) => l.type === 'remove').length;
      expect(a.additions).toBe(addsByLine);
      expect(a.deletions).toBe(delsByLine);
    }
  });
});
