import { QueryClient } from '@tanstack/solid-query';
import { queryClientOptions, setQueryClientForTests } from '@/lib/query/client';
import { resetBusForTests } from '@/lib/bus';
import { resetSseForTests } from '@/lib/query/sse';
import { createMockFetch, type MockFetch, type MockFetchAnswer } from './mock-fetch';

/**
 * Builds the client of one test.
 *
 * It carries the defaults of the app, so a test proves the behaviour the app
 * has, with one change: `gcTime` is zero. The app holds an unused entry for
 * five minutes, which in a test is a timer that outlives the case and a cache
 * that answers the case after it.
 */
export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    ...queryClientOptions,
    defaultOptions: {
      ...queryClientOptions.defaultOptions,
      queries: { ...queryClientOptions.defaultOptions?.queries, gcTime: 0 },
    },
  });
}

/**
 * Removes the three module singletons a test writes to.
 *
 * The order matters: a live stream routes an event into the cache, so the
 * streams close before the client goes.
 */
function resetQuerySingletons(client: QueryClient): void {
  resetSseForTests();
  resetBusForTests();
  client.clear();
  setQueryClientForTests(null);
}

/**
 * Runs the body against a fresh `QueryClient`, then puts the app's client back.
 *
 * Every hook and every stream route reads `getQueryClient()`, so the injection
 * reaches all of them without a Provider. The restore also closes every stream
 * the body opened and removes every bus handler it added, because one test
 * that leaves a handler behind answers the events of the next one.
 *
 * @example
 * await withQueryClient(async (client) => {
 *   client.setQueryData(keys.kilns(), [{ name: 'main' }]);
 *   ...
 * });
 */
export async function withQueryClient<T>(fn: (client: QueryClient) => T | Promise<T>): Promise<T> {
  const client = createTestQueryClient();
  setQueryClientForTests(client);
  try {
    return await fn(client);
  } finally {
    resetQuerySingletons(client);
  }
}

/** The client, the fetch and the removal of both. */
export interface TestQueryEnv {
  /** The client `getQueryClient()` answers until `restore` runs. */
  client: QueryClient;
  /** The mock `global.fetch` holds until `restore` runs. */
  fetch: MockFetch;
  /** Puts the app's client and the real `fetch` back. Safe to run twice. */
  restore(): void;
}

/**
 * Installs a fresh client and a mock `fetch` together, for a test that needs
 * both and cannot nest its body inside `withQueryClient` — a suite that sets
 * them up in `beforeEach`, for one.
 *
 * The caller runs `restore` in `afterEach`. A test that can nest its body
 * calls `withQueryClient` instead, which restores on its own.
 */
export function createTestQueryEnv(routes: Record<string, MockFetchAnswer> = {}): TestQueryEnv {
  const client = createTestQueryClient();
  const mockFetch = createMockFetch(routes);
  const previousFetch = global.fetch;

  setQueryClientForTests(client);
  global.fetch = mockFetch;

  let restored = false;
  return {
    client,
    fetch: mockFetch,
    restore(): void {
      if (restored) return;
      restored = true;
      global.fetch = previousFetch;
      resetQuerySingletons(client);
    },
  };
}
