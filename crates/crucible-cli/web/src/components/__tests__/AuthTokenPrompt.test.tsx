import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { AuthTokenPrompt } from '../AuthTokenPrompt';
import { getApiToken, setApiToken } from '@/lib/api';

beforeEach(() => {
  localStorage.clear();
  setApiToken(null);
  document.body.innerHTML = '';
});

describe('AuthTokenPrompt', () => {
  it('stays hidden until the api layer reports 401', () => {
    render(() => <AuthTokenPrompt onSaved={() => {}} />);
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();

    window.dispatchEvent(new CustomEvent('crucible:auth-required'));
    expect(screen.getByTestId('auth-token-prompt')).toBeInTheDocument();
  });

  it('saves the pasted token and invokes onSaved', () => {
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.input(screen.getByTestId('auth-token-input'), {
      target: { value: '  my-secret-key  ' },
    });
    fireEvent.click(screen.getByTestId('auth-token-save'));

    expect(getApiToken()).toBe('my-secret-key');
    expect(onSaved).toHaveBeenCalledOnce();
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();
  });

  it('does not save an empty token', () => {
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.click(screen.getByTestId('auth-token-save'));
    expect(getApiToken()).toBeNull();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('cancel dismisses without touching the stored token', () => {
    setApiToken('existing');
    render(() => <AuthTokenPrompt onSaved={() => {}} />);
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));

    fireEvent.click(screen.getByText('Cancel'));
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();
    expect(getApiToken()).toBe('existing');
  });
});
