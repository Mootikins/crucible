import { describe, it, expect } from 'vitest';
import { extractDiffFromToolCall, applyToolDiff } from '../tool-diffs';
import type { ToolCallDisplay } from '../types';

function call(overrides: Partial<ToolCallDisplay>): ToolCallDisplay {
  return {
    id: 'id-1',
    name: 'Edit',
    args: '',
    status: 'complete',
    ...overrides,
  };
}

describe('extractDiffFromToolCall — Edit', () => {
  it('extracts single diff from Edit args', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'Edit',
        args: JSON.stringify({
          file_path: 'src/foo.rs',
          old_string: 'fn old()',
          new_string: 'fn new()',
        }),
      }),
    );
    expect(result).toEqual({
      kind: 'single',
      fileName: 'src/foo.rs',
      oldContent: 'fn old()',
      newContent: 'fn new()',
    });
  });

  it('is case-insensitive on tool name', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'edit',
        args: JSON.stringify({ file_path: 'a', old_string: 'x', new_string: 'y' }),
      }),
    );
    expect(result?.kind).toBe('single');
  });
});

describe('extractDiffFromToolCall — Write', () => {
  it('extracts single diff with empty oldContent', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'Write',
        args: JSON.stringify({ file_path: 'src/new.ts', content: 'hello' }),
      }),
    );
    expect(result).toEqual({
      kind: 'single',
      fileName: 'src/new.ts',
      oldContent: '',
      newContent: 'hello',
    });
  });
});

describe('extractDiffFromToolCall — MultiEdit', () => {
  it('extracts multi diff with N edits', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'MultiEdit',
        args: JSON.stringify({
          file_path: 'src/foo.rs',
          edits: [
            { old_string: 'a', new_string: 'b' },
            { old_string: 'c', new_string: 'd' },
          ],
        }),
      }),
    );
    expect(result).toEqual({
      kind: 'multi',
      fileName: 'src/foo.rs',
      edits: [
        { oldContent: 'a', newContent: 'b' },
        { oldContent: 'c', newContent: 'd' },
      ],
    });
  });

  it('returns null when edits array is empty', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'MultiEdit',
        args: JSON.stringify({ file_path: 'x', edits: [] }),
      }),
    );
    expect(result).toBeNull();
  });
});

describe('extractDiffFromToolCall — defensive handling', () => {
  it('returns null when status is running', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'Edit',
        status: 'running',
        args: JSON.stringify({ file_path: 'a', old_string: 'x', new_string: 'y' }),
      }),
    );
    expect(result).toBeNull();
  });

  it('returns null for malformed JSON args', () => {
    const result = extractDiffFromToolCall(call({ name: 'Edit', args: '{not json' }));
    expect(result).toBeNull();
  });

  it('returns null for empty args', () => {
    const result = extractDiffFromToolCall(call({ name: 'Edit', args: '' }));
    expect(result).toBeNull();
  });

  it('returns null when required fields are missing (Edit)', () => {
    const result = extractDiffFromToolCall(
      call({ name: 'Edit', args: JSON.stringify({ file_path: 'a' }) }),
    );
    expect(result).toBeNull();
  });

  it('returns null when required fields are missing (Write)', () => {
    const result = extractDiffFromToolCall(
      call({ name: 'Write', args: JSON.stringify({ file_path: 'a' }) }),
    );
    expect(result).toBeNull();
  });

  it('returns null when fields are wrong type', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'Edit',
        args: JSON.stringify({ file_path: 'a', old_string: 1, new_string: 2 }),
      }),
    );
    expect(result).toBeNull();
  });

  it('returns null for unknown tool name', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'SomeRandomTool',
        args: JSON.stringify({ file_path: 'a', old_string: 'x', new_string: 'y' }),
      }),
    );
    expect(result).toBeNull();
  });

  it('accepts error status (so failed edits still show their attempted diff)', () => {
    const result = extractDiffFromToolCall(
      call({
        name: 'Edit',
        status: 'error',
        args: JSON.stringify({ file_path: 'a', old_string: 'x', new_string: 'y' }),
      }),
    );
    expect(result?.kind).toBe('single');
  });
});

describe('applyToolDiff — proposed content for the editor overlay', () => {
  const diffOf = (name: string, args: unknown) =>
    extractDiffFromToolCall(call({ name, args: JSON.stringify(args) }))!;

  it('Write overwrites the whole file', () => {
    const d = diffOf('Write', { file_path: '/a.ts', content: 'NEW\n' });
    expect(applyToolDiff('anything at all', d)).toBe('NEW\n');
  });

  it('Edit replaces the first occurrence in context', () => {
    const d = diffOf('Edit', { file_path: '/a.ts', old_string: 'let x = 1;', new_string: 'let x = 42;' });
    expect(applyToolDiff('top\nlet x = 1;\nbottom\n', d)).toBe('top\nlet x = 42;\nbottom\n');
  });

  it('Edit whose old_string is absent leaves content unchanged (already applied)', () => {
    const d = diffOf('Edit', { file_path: '/a.ts', old_string: 'gone', new_string: 'new' });
    expect(applyToolDiff('no match here', d)).toBe('no match here');
  });

  it('MultiEdit applies edits sequentially', () => {
    const d = diffOf('MultiEdit', {
      file_path: '/a.ts',
      edits: [
        { old_string: 'a', new_string: 'A' },
        { old_string: 'b', new_string: 'B' },
      ],
    });
    expect(applyToolDiff('a b c', d)).toBe('A B c');
  });

  it('MultiEdit skips an edit whose old_string is gone but still applies the rest', () => {
    const d = diffOf('MultiEdit', {
      file_path: '/a.ts',
      edits: [
        { old_string: 'missing', new_string: 'X' },
        { old_string: 'b', new_string: 'B' },
      ],
    });
    expect(applyToolDiff('a b c', d)).toBe('a B c');
  });

  it('replaces only the FIRST occurrence, matching daemon edit semantics', () => {
    const d = diffOf('Edit', { file_path: '/a.ts', old_string: 'x', new_string: 'Y' });
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
    const d = diffOf('Edit', { file_path: '/a.ts', old_string: 'PLACEHOLDER', new_string: replacement });
    expect(applyToolDiff('before\nPLACEHOLDER\nafter', d)).toBe(`before\n${replacement}\nafter`);
  });

  it('does not treat regex metacharacters in old_string as a pattern', () => {
    const d = diffOf('Edit', { file_path: '/a.ts', old_string: 'a.c', new_string: 'ok' });
    // 'abc' would match if old_string were a regex; only the literal 'a.c' may.
    expect(applyToolDiff('abc a.c', d)).toBe('abc ok');
  });
});
