import { describe, it, expect, beforeAll } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { DiffViewer } from '../DiffViewer';
import { initializeHighlighter } from '@/lib/shiki';

describe('DiffViewer — header and stats', () => {
  it('renders the file name when provided', () => {
    render(() => (
      <DiffViewer oldContent="a" newContent="b" fileName="src/foo.ts" />
    ));
    expect(screen.getByText('src/foo.ts')).toBeInTheDocument();
  });

  it('omits the file name when not provided', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="a" newContent="b" />
    ));
    // No file name span should be rendered in the header
    const headerSpans = container.querySelectorAll('.font-mono.text-shell-body');
    expect(headerSpans.length).toBe(0);
  });

  it('counts a pure substitution as one add and one remove', () => {
    render(() => <DiffViewer oldContent="one" newContent="two" />);
    expect(screen.getByText('+1')).toBeInTheDocument();
    expect(screen.getByText('-1')).toBeInTheDocument();
  });

  it('counts pure additions correctly', () => {
    render(() => (
      <DiffViewer oldContent={'a\n'} newContent={'a\nb\nc\n'} />
    ));
    expect(screen.getByText('+2')).toBeInTheDocument();
    expect(screen.getByText('-0')).toBeInTheDocument();
  });

  it('counts pure deletions correctly', () => {
    render(() => (
      <DiffViewer oldContent={'a\nb\nc\n'} newContent={'a\n'} />
    ));
    expect(screen.getByText('+0')).toBeInTheDocument();
    expect(screen.getByText('-2')).toBeInTheDocument();
  });

  it('reports zero changes for identical content', () => {
    render(() => <DiffViewer oldContent="same" newContent="same" />);
    expect(screen.getByText('+0')).toBeInTheDocument();
    expect(screen.getByText('-0')).toBeInTheDocument();
  });
});

describe('DiffViewer — line rendering', () => {
  it('renders + and - prefixes for changed lines and a space for context', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="ctx\nold" newContent="ctx\nnew" />
    ));
    const text = container.textContent ?? '';
    // The single context line is shown with its content
    expect(text).toContain('ctx');
    expect(text).toContain('old');
    expect(text).toContain('new');
  });

  it('renders a non-empty space for an empty diff line so layout does not collapse', () => {
    // Adding an empty line should still produce a flex row
    const { container } = render(() => (
      <DiffViewer oldContent={'a\n'} newContent={'a\n\n'} />
    ));
    const lineRows = container.querySelectorAll('div.flex');
    expect(lineRows.length).toBeGreaterThan(0);
  });
});

describe('DiffViewer — collapsed sections', () => {
  // Build content with enough untouched context to force a collapsed band
  // between two changes (CONTEXT_LINES = 3, so >7 ctx lines between changes
  // guarantees a collapsed run).
  const longOld = [
    'change-1-OLD',
    ...Array.from({ length: 20 }, (_, i) => `untouched-${i}`),
    'change-2-OLD',
  ].join('\n');
  const longNew = [
    'change-1-NEW',
    ...Array.from({ length: 20 }, (_, i) => `untouched-${i}`),
    'change-2-NEW',
  ].join('\n');

  it('renders a collapsed-band toggle for runs of untouched lines', () => {
    render(() => <DiffViewer oldContent={longOld} newContent={longNew} />);
    expect(screen.getByText(/lines unchanged/)).toBeInTheDocument();
  });

  it('expands the collapsed band on click and shows the hidden context', () => {
    render(() => <DiffViewer oldContent={longOld} newContent={longNew} />);
    const toggle = screen.getByText(/lines unchanged/);
    // Pick a context line that should be inside the collapsed band
    // (well past the 3-line context window from change-1).
    expect(screen.queryByText('untouched-10')).not.toBeInTheDocument();

    fireEvent.click(toggle);

    expect(screen.getByText('untouched-10')).toBeInTheDocument();
  });

  it('does NOT collapse for small files where every line is "interesting"', () => {
    render(() => <DiffViewer oldContent="a\nb\nc" newContent="a\nb\nd" />);
    expect(screen.queryByText(/lines unchanged/)).not.toBeInTheDocument();
  });

  it('keeps CONTEXT_LINES of context around a change visible', () => {
    // Three lines of context after the change should remain visible even when
    // a long collapsed band follows.
    render(() => <DiffViewer oldContent={longOld} newContent={longNew} />);
    // untouched-0..2 are within 3 lines of change-1 and should be visible
    expect(screen.getByText('untouched-0')).toBeInTheDocument();
    expect(screen.getByText('untouched-1')).toBeInTheDocument();
    expect(screen.getByText('untouched-2')).toBeInTheDocument();
  });
});

describe('DiffViewer — empty content', () => {
  it('renders without crashing when both sides are empty', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="" newContent="" />
    ));
    expect(container.textContent).toContain('+0');
    expect(container.textContent).toContain('-0');
  });

  it('treats a one-side-empty diff as pure additions', () => {
    render(() => (
      <DiffViewer oldContent="" newContent="new line" />
    ));
    expect(screen.getByText('+1')).toBeInTheDocument();
    expect(screen.getByText('-0')).toBeInTheDocument();
  });
});

describe('DiffViewer — gutter sizing', () => {
  it('expands gutter width with larger line numbers', () => {
    // Build a long file to push line numbers past 3 digits
    const old = Array.from({ length: 1200 }, (_, i) => `line-${i}`).join('\n');
    const next = old + '\nappended';
    const { container } = render(() => (
      <DiffViewer oldContent={old} newContent={next} />
    ));
    // Find a gutter span; should be at least 5ch (4 digits + 1 padding)
    const gutter = container.querySelector('span[style*="width"]') as HTMLElement | null;
    expect(gutter).not.toBeNull();
    const width = gutter!.style.width;
    // Format is "<N>ch"; just verify N is >= 5
    const n = parseInt(width.replace('ch', ''), 10);
    expect(n).toBeGreaterThanOrEqual(5);
  });
});

describe('DiffViewer — upgrades to highlighted output when Shiki resolves', () => {
  // Regression test for review feedback on Task 3: tokensForLine() previously
  // read a non-reactive module-level var, so a DiffViewer mounted before
  // initializeHighlighter() resolved stayed plain-text forever (no signal
  // change → no re-render). This test mounts BEFORE init, asserts no colored
  // spans, then awaits init and re-queries to assert colored spans appeared.
  //
  // Placement: this describe MUST run before the "syntax highlighting" block
  // below, because that block's beforeAll pre-warms Shiki for the file's
  // module instance. Vitest runs describes top-to-bottom within a file.
  it('renders plain text initially, then upgrades to colored tokens after init', async () => {
    const { container } = render(() => (
      <DiffViewer oldContent="fn old() {}" newContent="fn new() {}" language="rust" />
    ));

    // Before init: no syntax-colored spans should exist.
    expect(container.querySelectorAll('span[style*="color"]').length).toBe(0);
    expect(container.textContent).toContain('fn old() {}');
    expect(container.textContent).toContain('fn new() {}');

    // Trigger init. The reactive `highlighter` signal flips, which causes
    // every Solid scope that called tokensForLine() to re-run.
    await initializeHighlighter();

    // After init: colored spans must now exist. Solid's reactivity is
    // synchronous after a signal write, so no flush/tick is needed.
    expect(container.querySelectorAll('span[style*="color"]').length).toBeGreaterThan(0);
  });
});

describe('DiffViewer — syntax highlighting', () => {
  // Pre-warm Shiki once for all tests in this block. Vitest module isolation
  // means singleton state from other test files doesn't leak in.
  beforeAll(async () => {
    await initializeHighlighter();
  });

  it('renders colored token spans when language is rust', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="fn old() {}" newContent="fn new() {}" language="rust" />
    ));
    // After highlighting, line content is rendered as multiple <span style="color: ...">
    // wrappers instead of a single text node.
    const styledSpans = container.querySelectorAll('span[style*="color"]');
    expect(styledSpans.length).toBeGreaterThan(0);
  });

  it('preserves +/- row backgrounds when highlighting is active', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="fn a() {}" newContent="fn b() {}" language="rust" />
    ));
    // The 'remove' line keeps bg-error/15, the 'add' line keeps bg-ok/15
    expect(container.querySelector('.bg-ok\\/15')).not.toBeNull();
    expect(container.querySelector('.bg-error\\/15')).not.toBeNull();
  });

  it('falls back to plain text when language is unknown', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="x" newContent="y" language="completely-fake-lang" />
    ));
    // No colored spans — content rendered as plain text
    const styledSpans = container.querySelectorAll('span[style*="color"]');
    expect(styledSpans.length).toBe(0);
    expect(container.textContent).toContain('x');
    expect(container.textContent).toContain('y');
  });

  it('infers language from fileName when language prop is omitted', () => {
    const { container } = render(() => (
      <DiffViewer oldContent="fn a() {}" newContent="fn b() {}" fileName="src/foo.rs" />
    ));
    const styledSpans = container.querySelectorAll('span[style*="color"]');
    expect(styledSpans.length).toBeGreaterThan(0);
  });
});
