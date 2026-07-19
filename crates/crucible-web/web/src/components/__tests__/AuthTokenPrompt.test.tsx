import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

const loginMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  login: (...args: unknown[]) => loginMock(...args),
}));

import { AuthTokenPrompt } from '../AuthTokenPrompt';

beforeEach(() => {
  vi.clearAllMocks();
  document.body.innerHTML = '';
});

describe('AuthTokenPrompt', () => {
  it('stays hidden until the api layer reports 401', () => {
    render(() => <AuthTokenPrompt onSaved={() => {}} />);
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();

    window.dispatchEvent(new CustomEvent('crucible:auth-required'));
    expect(screen.getByTestId('auth-token-prompt')).toBeInTheDocument();
  });

  it('exchanges the pasted key via login() and invokes onSaved on success', async () => {
    loginMock.mockResolvedValue(true);
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.input(screen.getByTestId('auth-token-input'), {
      target: { value: '  my-secret-key  ' },
    });
    fireEvent.click(screen.getByTestId('auth-token-save'));

    await waitFor(() => {
      expect(onSaved).toHaveBeenCalledOnce();
    });
    expect(loginMock).toHaveBeenCalledWith('my-secret-key');
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();
  });

  it('shows a rejection message and stays open when the server refuses the key', async () => {
    loginMock.mockResolvedValue(false);
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.input(screen.getByTestId('auth-token-input'), {
      target: { value: 'wrong-key' },
    });
    fireEvent.click(screen.getByTestId('auth-token-save'));

    await waitFor(() => {
      expect(screen.getByTestId('auth-token-rejected')).toBeInTheDocument();
    });
    expect(onSaved).not.toHaveBeenCalled();
    expect(screen.getByTestId('auth-token-prompt')).toBeInTheDocument();
  });

  it('does not submit an empty key', () => {
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.click(screen.getByTestId('auth-token-save'));
    expect(loginMock).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('cancel dismisses without calling login', () => {
    render(() => <AuthTokenPrompt onSaved={() => {}} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.click(screen.getByText('Cancel'));
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();
    expect(loginMock).not.toHaveBeenCalled();
  });
});
