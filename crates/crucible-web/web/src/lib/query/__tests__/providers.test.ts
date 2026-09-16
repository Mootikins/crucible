import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { ProviderInfo } from '@/lib/types';
import { useProviders } from '../providers';

const LIST = 'GET /api/providers';

/** The storage key `swrLocal('providers')` wrote, which the hook keeps. */
const STORAGE_KEY = 'crucible:cache:providers';

function provider(name: string, over: Partial<ProviderInfo> = {}): ProviderInfo {
  return {
    name,
    provider_type: name,
    available: true,
    default_model: `${name}/default`,
    models: [`${name}/one`],
    is_local: false,
    ...over,
  };
}

/** The envelope `GET /api/providers` answers; `listProviders` unwraps it. */
function body(providers: ProviderInfo[]): { providers: ProviderInfo[] } {
  return { providers };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

beforeEach(() => {
  localStorage.removeItem(STORAGE_KEY);
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
  localStorage.removeItem(STORAGE_KEY);
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useProviders', () => {
  it('deduplicates reads', async () => {
    // The session context probed the providers on start and the composer
    // probed them again on mount. The probe asks every configured provider
    // whether it answers, so it is one of the slowest reads in the shell.
    env = createTestQueryEnv({ [LIST]: () => body([provider('openai')]) });

    const both = inRoot(() => ({ context: useProviders(), composer: useProviders() }));

    await vi.waitFor(() => expect(both.context.data).toEqual([provider('openai')]));
    expect(both.composer.data).toEqual([provider('openai')]);
    expect(env.fetch.calls(LIST)).toBe(1);
  });

  it('paints the stored list before the probe answers, then corrects it', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([provider('stored')]));
    let release: () => void = () => {};
    const answered = new Promise<void>((resolve) => (release = resolve));
    env = createTestQueryEnv({
      [LIST]: async () => {
        await answered;
        return body([provider('live')]);
      },
    });

    const query = inRoot(() => useProviders());

    await vi.waitFor(() => expect(query.data).toEqual([provider('stored')]));
    release();
    await vi.waitFor(() => expect(query.data).toEqual([provider('live')]));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([provider('live')]);
  });

  it('reaches the caller with the refusal instead of an empty list', async () => {
    // "No providers configured" and "the daemon refused the question" are two
    // different screens, and `swrLocal` showed the first for both.
    env = createTestQueryEnv({ [LIST]: apiError(500, 'provider registry is down') });

    const query = inRoot(() => useProviders());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to list providers');
    expect(query.data).toBeUndefined();
  });
});
