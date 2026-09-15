import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { EmptyState } from '@/components/ui/EmptyState';

describe('EmptyState', () => {
  it('draws no icon: every empty pane reads the same way', () => {
    const { getByTestId } = render(() => <EmptyState title="No background tasks" />);

    const state = getByTestId('empty-state');
    expect(state.querySelector('svg')).toBeNull();
    expect(state.querySelector('.empty-state-title')!.textContent).toContain('No background tasks');
  });

  it('renders the body and defaults to the empty tone', () => {
    const { getByTestId } = render(() => (
      <EmptyState title="No background tasks" body="Tasks appear here." />
    ));

    const root = getByTestId('empty-state');
    expect(root.getAttribute('data-tone')).toBe('empty');
    expect(root.textContent).toContain('Tasks appear here.');
    // An empty result is not an alert. Only the error tone announces itself.
    expect(root.getAttribute('role')).toBeNull();
  });

  it('marks the error tone and announces it', () => {
    const { getByTestId } = render(() => (
      <EmptyState tone="error" title="Backlinks are unavailable" />
    ));

    const root = getByTestId('empty-state');
    expect(root.getAttribute('data-tone')).toBe('error');
    expect(root.getAttribute('role')).toBe('alert');
  });

  it('runs one action and carries its shortcut', () => {
    const onClick = vi.fn();
    const { getAllByTestId } = render(() => (
      <EmptyState title="No sessions yet" action={{ label: 'New session', onClick, kbd: '⌘N' }} />
    ));

    const buttons = getAllByTestId('empty-state-action');
    expect(buttons).toHaveLength(1);
    expect(buttons[0].textContent).toContain('New session');
    expect(buttons[0].textContent).toContain('⌘N');
    // Every interactive element takes the one focus treatment.
    expect(buttons[0].className).toContain('focus-ring');

    fireEvent.click(buttons[0]);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('renders two actions in order', () => {
    const first = vi.fn();
    const second = vi.fn();
    const { getAllByTestId } = render(() => (
      <EmptyState
        title="No note is open"
        action={[
          { label: 'Open a note', onClick: first },
          { label: 'Start a session', onClick: second },
        ]}
      />
    ));

    const buttons = getAllByTestId('empty-state-action');
    expect(buttons.map((b) => b.textContent)).toEqual(['Open a note', 'Start a session']);

    fireEvent.click(buttons[1]);
    expect(second).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();
  });

  it('renders no action row when it has no action', () => {
    const { queryAllByTestId } = render(() => <EmptyState title="Nothing" />);
    expect(queryAllByTestId('empty-state-action')).toHaveLength(0);
  });
});
