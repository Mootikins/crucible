import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listProviders, listSessions, searchSessions } from '@/lib/api';

/**
 * What the client does when a reply is not the shape it expected.
 *
 * A reverse proxy's error page served as 200, a daemon one field-rename
 * ahead, a truncated body — all of these used to surface as
 * `Cannot read properties of undefined (reading 'map')`, which names neither
 * the call that failed nor anything an operator can act on.
 */
const ok = (body: unknown) =>
  Promise.resolve(
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }),
  );

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn());
});

describe('a reply whose list is missing', () => {
  it('names the call and the field, not a property of undefined', async () => {
    vi.mocked(fetch).mockReturnValue(ok({ total: 0 }));
    await expect(listSessions()).rejects.toThrow(/Failed to list sessions.*"sessions"/);
  });

  // This route answers a BARE array, so the failure shape differs and the
  // guard has to sit on the value itself rather than on a field.
  it('does the same for a search, whose matches ride under a field', async () => {
    vi.mocked(fetch).mockReturnValue(ok({}));
    await expect(searchSessions('x')).rejects.toThrow(/Failed to search sessions.*"matches"/);
  });

  it('does the same for providers', async () => {
    vi.mocked(fetch).mockReturnValue(ok({}));
    await expect(listProviders()).rejects.toThrow(/Failed to list providers.*"providers"/);
  });

  // A body that parsed but is the wrong TYPE is the proxy-error-page case.
  it('refuses a value that is not an array at all', async () => {
    vi.mocked(fetch).mockReturnValue(ok({ sessions: 'nope' }));
    await expect(listSessions()).rejects.toThrow(/"sessions"/);
  });

  it('still answers normally when the list is there', async () => {
    vi.mocked(fetch).mockReturnValue(ok({ providers: [{ id: 'a', name: 'A' }] }));
    expect(await listProviders()).toHaveLength(1);
  });

  it('accepts an empty list as an answer, not as a failure', async () => {
    vi.mocked(fetch).mockReturnValue(ok({ sessions: [], total: 0 }));
    expect(await listSessions()).toEqual([]);
  });
});
