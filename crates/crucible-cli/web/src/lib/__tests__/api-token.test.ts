import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { setApiToken, getApiToken, withAccessToken, getConfig } from '../api';

describe('API token handling', () => {
  beforeEach(() => {
    localStorage.clear();
    setApiToken(null);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    setApiToken(null);
  });

  it('withAccessToken appends the token only when one is set', () => {
    expect(withAccessToken('/api/chat/events/s1')).toBe('/api/chat/events/s1');

    setApiToken('secret-key');
    expect(withAccessToken('/api/chat/events/s1')).toBe(
      '/api/chat/events/s1?access_token=secret-key'
    );
    expect(withAccessToken('/api/x?a=1')).toBe('/api/x?a=1&access_token=secret-key');
  });

  it('requests attach Authorization: Bearer when a token is set', async () => {
    setApiToken('secret-key');
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ kiln_path: '/k' }), { status: 200 }));

    await getConfig();

    const headers = new Headers((fetchMock.mock.calls[0][1] as RequestInit).headers);
    expect(headers.get('Authorization')).toBe('Bearer secret-key');
  });

  it('requests carry no Authorization header without a token', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ kiln_path: '/k' }), { status: 200 }));

    await getConfig();

    const headers = new Headers((fetchMock.mock.calls[0][1] as RequestInit).headers);
    expect(headers.get('Authorization')).toBeNull();
  });

  it('401 without a token surfaces the ?token= bootstrap hint', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 401 }));

    await expect(getConfig()).rejects.toThrow(/\?token=<api key>/);
  });

  it('setApiToken persists to and clears localStorage', () => {
    setApiToken('abc');
    expect(localStorage.getItem('crucible_api_token')).toBe('abc');
    expect(getApiToken()).toBe('abc');

    setApiToken(null);
    expect(localStorage.getItem('crucible_api_token')).toBeNull();
    expect(getApiToken()).toBeNull();
  });
});
