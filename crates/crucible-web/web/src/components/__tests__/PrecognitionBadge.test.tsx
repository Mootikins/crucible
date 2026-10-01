import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { PrecognitionBadge } from '../PrecognitionBadge';

const openNote = vi.hoisted(() => vi.fn());
vi.mock('@/lib/note-actions', () => ({ openNoteInEditor: openNote }));

describe('PrecognitionBadge', () => {
  it('opens a recalled note using the owning session kiln', () => {
    render(() => <PrecognitionBadge notesCount={1} notes={[{ name: 'Guides/Start', relevance: 0.9 }]} kiln="/session-kiln" />);
    fireEvent.click(screen.getByTestId('precognition-badge-toggle'));
    fireEvent.click(screen.getByRole('button', { name: 'Guides/Start' }));
    expect(openNote).toHaveBeenCalledWith('Guides/Start', '/session-kiln');
  });

  it('renders the note count' , () => {
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

  it('lists one note per line with the score rounded to two digits', () => {
    const notes = [
      { name: 'Kilns', relevance: 0.8345671234 },
      { name: 'Wikilinks', relevance: 0.7150001 },
    ];
    render(() => <PrecognitionBadge notesCount={2} notes={notes} />);
    fireEvent.click(screen.getByTestId('precognition-badge-toggle'));

    const rows = screen.getByTestId('precognition-badge-notes').querySelectorAll('li');
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toBe('Kilns0.83');
    expect(rows[1].textContent).toBe('Wikilinks0.72');
  });
});
