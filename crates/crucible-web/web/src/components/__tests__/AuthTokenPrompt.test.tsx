import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { createMockFetch, type MockFetch } from '@/test-utils/mock-fetch';
import { getBus, resetBusForTests } from '@/lib/bus';

import { AuthTokenPrompt } from '../AuthTokenPrompt';

/**
 * The exchange reaches the daemon on the WIRE.
 *
 * Signing in is deliberately NOT a cache entry — it mints an HttpOnly cookie
 * and the page reloads — so there is no hook to read here. What there is is
 * one `POST /api/auth/login`, and the key the prompt sends is the whole
 * contract: a mocked module would prove that the component called a function,
 * not that the daemon was sent the key the user pasted.
 */
let fetchMock: MockFetch;
let realFetch: typeof fetch;
/** Whether the daemon accepts the next key. */
let accepts = true;
/** Every key the daemon was sent, in order. */
let sent: string[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  document.body.innerHTML = '';
  accepts = true;
  sent = [];
  realFetch = global.fetch;
  fetchMock = createMockFetch({
    'POST /api/auth/login': async (request: Request) => {
      sent.push(((await request.json()) as { key: string }).key);
      return new Response('', { status: accepts ? 200 : 401 });
    },
  });
  global.fetch = fetchMock;
});

afterEach(() => {
  global.fetch = realFetch;
  // The prompt subscribes on render; a handler left behind would open the
  // modal of the next test.
  resetBusForTests();
});

describe('AuthTokenPrompt', () => {
  it('stays hidden until the api layer reports 401', () => {
    render(() => <AuthTokenPrompt onSaved={() => {}} />);
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();

    getBus().emit('authRequired', {});
    expect(screen.getByTestId('auth-token-prompt')).toBeInTheDocument();
  });

  it('exchanges the pasted key via login() and invokes onSaved on success', async () => {
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    getBus().emit('authRequired', {});

    fireEvent.input(screen.getByTestId('auth-token-input'), {
      target: { value: '  my-secret-key  ' },
    });
    fireEvent.click(screen.getByTestId('auth-token-save'));

    await waitFor(() => {
      expect(onSaved).toHaveBeenCalledOnce();
    });
    expect(sent).toEqual(['my-secret-key']);
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();
  });

  it('shows a rejection message and stays open when the server refuses the key', async () => {
    accepts = false;
    const onSaved = vi.fn();
    render(() => <AuthTokenPrompt onSaved={onSaved} />);
    getBus().emit('authRequired', {});

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
    getBus().emit('authRequired', {});

    fireEvent.click(screen.getByTestId('auth-token-save'));
    expect(sent).toEqual([]);
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('cancel dismisses without calling login', () => {
    render(() => <AuthTokenPrompt onSaved={() => {}} />);
    getBus().emit('authRequired', {});

    fireEvent.click(screen.getByText('Cancel'));
    expect(screen.queryByTestId('auth-token-prompt')).not.toBeInTheDocument();
    expect(sent).toEqual([]);
  });
});
