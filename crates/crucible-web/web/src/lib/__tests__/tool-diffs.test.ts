import { describe, it, expect } from 'vitest';
import { extractDiffFromToolCall } from '../tool-diffs';
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
