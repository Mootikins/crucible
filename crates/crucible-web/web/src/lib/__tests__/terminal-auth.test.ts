import { describe, expect, it, vi } from 'vitest';
import { classifyTerminalAuth, isAuthStatus } from '../terminal-auth';

describe('isAuthStatus', () => {
  it.each([401, 403])('classifies %i as an auth failure', (status) => {
    expect(isAuthStatus(status)).toBe(true);
  });

  it.each([200, 204, 302, 404, 500])('classifies %i as not an auth failure', (status) => {
    expect(isAuthStatus(status)).toBe(false);
  });
});

describe('classifyTerminalAuth', () => {
  it('calls a refused probe an auth failure', async () => {
    const fetch = vi.fn().mockResolvedValue(
      new Response('{"error":{"code":401,"message":"Shell access requires the API key"}}', {
        status: 401,
      }),
    );
    await expect(classifyTerminalAuth('http://x/api/terminal/ws', fetch)).resolves.toBe('auth');
    expect(fetch).toHaveBeenCalledWith('http://x/api/terminal/ws', { method: 'GET' });
  });

  it('treats a 403 refusal the same as a 401', async () => {
    const fetch = vi.fn().mockResolvedValue(new Response('{}', { status: 403 }));
    await expect(classifyTerminalAuth('http://x/api/terminal/ws', fetch)).resolves.toBe('auth');
  });

  it('leaves an accepted probe to the reconnect loop', async () => {
    const fetch = vi.fn().mockResolvedValue(new Response('{}', { status: 200 }));
    await expect(classifyTerminalAuth('http://x/api/terminal/ws', fetch)).resolves.toBe(
      'transient',
    );
  });

  it('probes the http form of a ws:// endpoint, which fetch cannot load', async () => {
    const fetch = vi.fn().mockResolvedValue(new Response('{}', { status: 401 }));
    await expect(classifyTerminalAuth('ws://x/api/terminal/ws', fetch)).resolves.toBe('auth');
    expect(fetch).toHaveBeenCalledWith('http://x/api/terminal/ws', { method: 'GET' });
  });

  it('treats a dead network as transient, not a credential problem', async () => {
    const fetch = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
    await expect(classifyTerminalAuth('http://x/api/terminal/ws', fetch)).resolves.toBe(
      'transient',
    );
  });
});
