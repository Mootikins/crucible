import { describe, it, expect, beforeAll } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { ToolCard } from '../ToolCard';
import { initializeHighlighter } from '@/lib/shiki';
import type { ToolCallDisplay } from '@/lib/types';

// Integration test: render the REAL ToolCard with the REAL DiffViewer and
// MultiEditDiff (no vi.mock anywhere). This exercises the production path
// from a tool-call payload through arg parsing, diff extraction, and Shiki
// tokenization to the rendered DOM. ToolCard.test.tsx mocks the diff
// components, so without this file no test asserts that a real Edit/Write/
// MultiEdit call from an agent actually produces highlighted token spans.

function makeTool(overrides: Partial<ToolCallDisplay>): ToolCallDisplay {
  return {
    id: 'tc-int-1',
    name: 'Edit',
    args: '',
    status: 'complete',
    ...overrides,
  };
}

function expandCard(container: HTMLElement) {
  // Idempotent: only click if the header still shows the collapsed caret.
  // (Error-status cards auto-expand; we don't use those here, but matches
  // the helper pattern in ToolCard.test.tsx for consistency.)
  const button = container.querySelector('button');
  if (!button) return;
  if (!button.textContent?.includes('▼')) {
    fireEvent.click(button);
  }
}

describe('ToolCard integration — real DiffViewer + Shiki', () => {
  // Pre-warm Shiki once for the whole file. The reactive `highlighter`
  // signal flips inside this await, so by the time any test renders, every
  // subsequent tokensForLine() call returns colored tokens synchronously.
  beforeAll(async () => {
    await initializeHighlighter();
  });

  it('Edit on foo.rs renders a real highlighted diff (file name, +/- stats, colored spans, no Arguments JSON)', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'Edit',
          args: JSON.stringify({
            file_path: 'foo.rs',
            old_string: 'fn old() { 0 }',
            new_string: 'fn new() { 1 }',
          }),
          result: 'edited',
        })}
      />
    ));
    expandCard(container);

    // Diff header file name from the real DiffViewer
    expect(screen.getByText('foo.rs')).toBeInTheDocument();

    // +/- stats from analyzeDiff (one substitution: +1/-1)
    expect(screen.getByText('+1')).toBeInTheDocument();
    expect(screen.getByText('-1')).toBeInTheDocument();

    // Shiki produced at least one colored token span for rust source
    const styledSpans = container.querySelectorAll('span[style*="color"]');
    expect(styledSpans.length).toBeGreaterThan(0);

    // Arguments JSON <pre> must be suppressed when a diff is rendered
    // (Task 6 fix). The Arguments heading is the canonical marker.
    expect(screen.queryByText('Arguments')).not.toBeInTheDocument();
    expect(container.textContent).not.toContain('"old_string"');
    expect(container.textContent).not.toContain('"new_string"');
  });

  it('Write on new.py renders an all-additions diff with highlighting', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'Write',
          args: JSON.stringify({
            file_path: 'new.py',
            content: 'def hello():\n    return 42',
          }),
          result: 'wrote',
        })}
      />
    ));
    expandCard(container);

    // Outer header (DiffViewer's own) shows the file name
    expect(screen.getByText('new.py')).toBeInTheDocument();

    // Write maps to oldContent='' → analyzeDiff treats every newContent line
    // as an addition. Two lines → +2 / -0.
    expect(screen.getByText('+2')).toBeInTheDocument();
    expect(screen.getByText('-0')).toBeInTheDocument();

    // Python source should be tokenized by Shiki
    const styledSpans = container.querySelectorAll('span[style*="color"]');
    expect(styledSpans.length).toBeGreaterThan(0);
  });

  it('MultiEdit on mod.ts renders stacked diffs with combined stats and inner highlighting', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'MultiEdit',
          args: JSON.stringify({
            file_path: 'mod.ts',
            edits: [
              { old_string: 'const a = 1', new_string: 'const a = 2' },
              { old_string: 'const b = 3', new_string: 'const b = 4' },
            ],
          }),
          result: 'multi-edited',
        })}
      />
    ));
    expandCard(container);

    // Outer MultiEditDiff header shows the file path
    expect(screen.getByText('mod.ts')).toBeInTheDocument();

    // MultiEditDiff renders an "N edits" pill
    expect(screen.getByText('2 edits')).toBeInTheDocument();

    // Per-edit headers
    expect(screen.getByText('Edit 1 of 2')).toBeInTheDocument();
    expect(screen.getByText('Edit 2 of 2')).toBeInTheDocument();

    // Inner DiffViewers (one per edit) ran Shiki for typescript and produced
    // colored token spans.
    const styledSpans = container.querySelectorAll('span[style*="color"]');
    expect(styledSpans.length).toBeGreaterThan(0);
  });

  it('Bash falls back to plain <pre> result and keeps the Arguments JSON section', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'Bash',
          args: JSON.stringify({ command: 'ls' }),
          result: 'foo\nbar',
        })}
      />
    ));
    expandCard(container);

    // Result <pre> contains the command output. Use a regex match because
    // whitespace inside the <pre> can introduce stray surrounding text nodes
    // in some renderers; structural assertions below are the load-bearing ones.
    expect(container.textContent).toContain('foo');
    expect(container.textContent).toContain('bar');

    // Arguments JSON IS rendered for non-diff tools (no suppression path).
    expect(screen.getByText('Arguments')).toBeInTheDocument();
    expect(container.textContent).toContain('"command"');

    // No DiffViewer file-name header (zinc-300 mono span) means no diff is
    // present — Bash doesn't map to a diff and should not render one.
    const diffHeaderFileSpans = container.querySelectorAll('.font-mono.text-zinc-300');
    expect(diffHeaderFileSpans.length).toBe(0);
  });
});
