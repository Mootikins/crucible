import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { EmptyState } from '../EmptyState';

describe('EmptyState', () => {
  it('shows no action button when onAction is not provided', () => {
    render(() => <EmptyState />);
    expect(screen.getByText('No session open')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows the New Session button and fires onAction on click', () => {
    const onAction = vi.fn();
    render(() => <EmptyState onAction={onAction} />);

    const button = screen.getByRole('button', { name: /New Session/ });
    fireEvent.click(button);
    expect(onAction).toHaveBeenCalledOnce();
  });

  it('uses a custom action label when given', () => {
    render(() => <EmptyState onAction={() => {}} actionLabel="Create" />);
    expect(screen.getByRole('button', { name: /Create/ })).toBeInTheDocument();
  });
});
