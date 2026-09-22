import { describe, it, expect } from 'vitest';
import { toolDiffsFromWire } from '../tool-diffs';
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
    ).toEqual([
      { kind: 'single', fileName: 'src/foo.rs', oldContent: 'fn old()', newContent: 'fn new()' },
    ]);
  });

  it('maps a null old side to an empty one (whole-file write)', () => {
    expect(
      toolDiffsFromWire(
        wire({ diffs: [{ path: 'src/new.ts', old_content: null, new_content: 'hello' }] }),
      ),
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
    expect(toolDiffsFromWire(wire({ diffs: malformed as ToolCallDisplay['diffs'] }))).toEqual([
      { kind: 'single', fileName: 'ok', oldContent: 'x', newContent: 'y' },
    ]);
  });
});
