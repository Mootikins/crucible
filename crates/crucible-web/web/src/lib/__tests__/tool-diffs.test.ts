import { describe, it, expect } from 'vitest';
import { toolDiffsFromWire, applyToolDiff, type ToolDiff } from '../tool-diffs';
import type { ToolCallDisplay } from '../types';

type WireDiffs = ToolCallDisplay['diffs'];

function call(overrides: Partial<ToolCallDisplay>): ToolCallDisplay {
  return {
    id: 'tc-1',
    name: 'Edit',
    args: '{}',
    status: 'complete',
    ...overrides,
  };
}

function wire(overrides: Partial<ToolCallDisplay>): WireDiffs {
  return call(overrides).diffs;
}

describe('toolDiffsFromWire — converting the daemon projection', () => {
  it('converts a single edit diff', () => {
    expect(
      toolDiffsFromWire(
        wire({ diffs: [{ path: 'src/foo.rs', old_content: 'fn old()', new_content: 'fn new()' }] }),
      ),
    ).toEqual([{ kind: 'single', fileName: 'src/foo.rs', oldContent: 'fn old()', newContent: 'fn new()' }]);
  });

  it('maps a null old side to an empty one (whole-file write)', () => {
    expect(
      toolDiffsFromWire(wire({ diffs: [{ path: 'src/new.ts', old_content: null, new_content: 'hello' }] })),
    ).toEqual([{ kind: 'single', fileName: 'src/new.ts', oldContent: '', newContent: 'hello' }]);
  });

  it('merges several edits to one file into a multi diff', () => {
    expect(
      toolDiffsFromWire(
        wire({
          diffs: [
            { path: 'src/foo.rs', old_content: 'a', new_content: 'b' },
            { path: 'src/foo.rs', old_content: 'c', new_content: 'd' },
          ],
        }),
      ),
    ).toEqual([
      {
        kind: 'multi',
        fileName: 'src/foo.rs',
        edits: [
          { oldContent: 'a', newContent: 'b' },
          { oldContent: 'c', newContent: 'd' },
        ],
      },
    ]);
  });

  it('keeps distinct files as distinct diffs', () => {
    const result = toolDiffsFromWire(
      wire({
        diffs: [
          { path: 'src/a.rs', old_content: 'a', new_content: 'b' },
          { path: 'src/b.rs', old_content: null, new_content: 'new file' },
        ],
      }),
    );
    expect(result).toHaveLength(2);
    expect(result[0].fileName).toBe('src/a.rs');
    expect(result[1].fileName).toBe('src/b.rs');
  });

  it('returns no diffs when the event carries none', () => {
    expect(toolDiffsFromWire(wire({}))).toEqual([]);
    expect(toolDiffsFromWire(wire({ diffs: [] }))).toEqual([]);
  });

  it('is indifferent to status — a running or failed call shows its proposed diff', () => {
    for (const status of ['running', 'error', 'complete'] as const) {
      expect(
        toolDiffsFromWire(
          wire({ status, diffs: [{ path: 'a', old_content: 'x', new_content: 'y' }] }),
        ),
      ).toHaveLength(1);
    }
  });

  it('skips malformed entries instead of throwing', () => {
    // The wire is untyped JSON at this boundary; malformed entries are a
    // runtime reality the converter must tolerate.
    const malformed: unknown = [
      { path: 'ok', old_content: 'x', new_content: 'y' },
      { path: 42, old_content: null, new_content: 'nope' },
      { path: 'also-bad' },
    ];
    expect(
      toolDiffsFromWire(wire({ diffs: malformed as ToolCallDisplay['diffs'] })),
    ).toEqual([{ kind: 'single', fileName: 'ok', oldContent: 'x', newContent: 'y' }]);
  });
});

describe('applyToolDiff — proposed content for the editor overlay', () => {
  const diffOf = (
    kind: 'single' | 'multi',
    a: string,
    b: string,
  ): ToolDiff =>
    kind === 'single'
      ? { kind: 'single', fileName: '/a.ts', oldContent: a, newContent: b }
      : {
          kind: 'multi',
          fileName: '/a.ts',
          edits: JSON.parse(a).map((e: { old_string: string; new_string: string }) => ({
            oldContent: e.old_string,
            newContent: e.new_string,
          })),
        };

  it('Write overwrites the whole file', () => {
    const d = diffOf('single', '', 'NEW\n');
    expect(applyToolDiff('anything at all', d)).toBe('NEW\n');
  });

  it('Edit replaces the first occurrence in context', () => {
    const d = diffOf('single', 'let x = 1;', 'let x = 42;');
    expect(applyToolDiff('top\nlet x = 1;\nbottom\n', d)).toBe('top\nlet x = 42;\nbottom\n');
  });

  it('Edit whose old_string is absent leaves content unchanged (already applied)', () => {
    const d = diffOf('single', 'gone', 'new');
    expect(applyToolDiff('no match here', d)).toBe('no match here');
  });

  it('MultiEdit applies edits sequentially', () => {
    const d = diffOf(
      'multi',
      JSON.stringify([
        { old_string: 'a', new_string: 'A' },
        { old_string: 'b', new_string: 'B' },
      ]),
      '',
    );
    expect(applyToolDiff('a b c', d)).toBe('A B c');
  });

  it('MultiEdit skips an edit whose old_string is gone but still applies the rest', () => {
    const d = diffOf(
      'multi',
      JSON.stringify([
        { old_string: 'missing', new_string: 'X' },
        { old_string: 'b', new_string: 'B' },
      ]),
      '',
    );
    expect(applyToolDiff('a b c', d)).toBe('a B c');
  });

  it('replaces only the FIRST occurrence, matching daemon edit semantics', () => {
    const d = diffOf('single', 'x', 'Y');
    expect(applyToolDiff('x x x', d)).toBe('Y x x');
  });

  // Regression: String.replace(needle, replacement) interprets $&, $`, $', $n
  // and $$ in the REPLACEMENT. Real edits inserting shell/regex/jQuery code
  // hit this, and the corruption would land in the editor buffer.
  it.each([
    ['$&', 'echo "$&"'],
    ['$`', 'sed "$`"'],
    ["$'", "awk \"$'\""],
    ['$1', 'const g = m.replace(/(a)/, "$1!");'],
    ['$$', 'const cost = "$$5";'],
  ])('inserts %s literally instead of as a substitution pattern', (_label, replacement) => {
    const d = diffOf('single', 'PLACEHOLDER', replacement);
    expect(applyToolDiff('before\nPLACEHOLDER\nafter', d)).toBe(`before\n${replacement}\nafter`);
  });

  it('does not treat regex metacharacters in old_string as a pattern', () => {
    const d = diffOf('single', 'a.c', 'ok');
    // 'abc' would match if old_string were a regex; only the literal 'a.c' may.
    expect(applyToolDiff('abc a.c', d)).toBe('abc ok');
  });
});
