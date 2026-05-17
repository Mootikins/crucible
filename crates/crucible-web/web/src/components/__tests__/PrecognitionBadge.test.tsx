import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { PrecognitionBadge } from '../PrecognitionBadge';

describe('PrecognitionBadge', () => {
  it('renders the note count', () => {
    render(() => <PrecognitionBadge notesCount={3} notes={[]} />);
    expect(screen.getByText(/Enriched with 3 notes/)).toBeInTheDocument();
  });

  it('uses singular "note" when count is 1', () => {
    render(() => <PrecognitionBadge notesCount={1} notes={[]} />);
    expect(screen.getByText(/Enriched with 1 note$/)).toBeInTheDocument();
  });

  it('does not show expand caret when notes array is empty', () => {
    render(() => <PrecognitionBadge notesCount={0} notes={[]} />);
    fireEvent.click(screen.getByTestId('precognition-badge-toggle'));
    expect(screen.queryByTestId('precognition-badge-notes')).not.toBeInTheDocument();
  });

  it('expands to show notes on click and collapses again', () => {
    const notes = [
      { name: 'note-a', relevance: 0.91 },
      { name: 'note-b', relevance: 0.72 },
    ];
    render(() => <PrecognitionBadge notesCount={2} notes={notes} />);

    // Initially collapsed.
    expect(screen.queryByTestId('precognition-badge-notes')).not.toBeInTheDocument();

    // Expand.
    fireEvent.click(screen.getByTestId('precognition-badge-toggle'));
    const list = screen.getByTestId('precognition-badge-notes');
    expect(list).toBeInTheDocument();
    expect(list.textContent).toContain('note-a');
    expect(list.textContent).toContain('0.91');
    expect(list.textContent).toContain('note-b');

    // Collapse.
    fireEvent.click(screen.getByTestId('precognition-badge-toggle'));
    expect(screen.queryByTestId('precognition-badge-notes')).not.toBeInTheDocument();
  });
});
