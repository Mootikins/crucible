import { describe, it, expect, vi, afterEach } from 'vitest';
import { login, getConfig } from '../api';

describe('API auth (cookie session via /api/auth/login)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('login POSTs the key as JSON and reports acceptance', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(null, { status: 204 }));

    expect(await login('secret-key')).toBe(true);

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/auth/login');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual({ key: 'secret-key' });
  });

  it('login reports rejection on 401 and on network failure', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 401 }));
    expect(await login('wrong')).toBe(false);

    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('offline'));
    expect(await login('secret-key')).toBe(false);
  });

  it('the key never appears in a request URL', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(null, { status: 204 }));

    await login('secret-key');

    expect(String(fetchMock.mock.calls[0][0])).not.toContain('secret-key');
  });

  it('requests carry no Authorization header (cookie is the credential)', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ kiln_path: '/k' }), { status: 200 }));

    await getConfig();

    const init = fetchMock.mock.calls[0][1] as RequestInit;
    const headers = new Headers(init?.headers);
    expect(headers.get('Authorization')).toBeNull();
  });

  it('401 surfaces the sign-in hint', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 401 }));

    await expect(getConfig()).rejects.toThrow(/sign in with the API key/);
  });
});
