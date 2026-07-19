import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { MultiEditDiff } from '../MultiEditDiff';

describe('MultiEditDiff', () => {
  const edits = [
    { oldContent: 'one', newContent: 'two' },
    { oldContent: 'three\nfour', newContent: 'three\nFOUR' },
  ];

  it('renders the file name in the header', () => {
    render(() => <MultiEditDiff fileName="src/foo.rs" edits={edits} />);
    expect(screen.getByText('src/foo.rs')).toBeInTheDocument();
  });

  it('shows combined +/- stats summed across all edits', () => {
    render(() => <MultiEditDiff fileName="src/foo.rs" edits={edits} />);
    // Edit 1: +1/-1, Edit 2: +1/-1 → +2/-2
    expect(screen.getByText('+2')).toBeInTheDocument();
    expect(screen.getByText('-2')).toBeInTheDocument();
  });

  it('shows an "Edit N of M" label per edit', () => {
    render(() => <MultiEditDiff fileName="src/foo.rs" edits={edits} />);
    expect(screen.getByText('Edit 1 of 2')).toBeInTheDocument();
    expect(screen.getByText('Edit 2 of 2')).toBeInTheDocument();
  });

  it('renders one DiffViewer body per edit (no nested file headers)', () => {
    const { container } = render(() => (
      <MultiEditDiff fileName="src/foo.rs" edits={edits} />
    ));
    // Outer header has one .font-mono.text-shell-body (file name).
    // Inner DiffViewers should have none (hideHeader=true).
    const fileNameSpans = container.querySelectorAll('.font-mono.text-shell-body');
    expect(fileNameSpans.length).toBe(1);
  });
});
