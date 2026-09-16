import { QueryClient, type QueryClientConfig } from '@tanstack/solid-query';

/**
 * The defaults every client in the app and in the tests shares.
 *
 * `staleTime` is five minutes because the daemon pushes a change over SSE, so
 * a background refetch on focus or on remount would repeat a fetch the stream
 * already covers. `retry` is false because a failed read must reach the user
 * as an error, not as three silent attempts; a mutation names its own retry.
 */
export const queryClientOptions: QueryClientConfig = {
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000,
      retry: false,
      refetchOnWindowFocus: false,
    },
    mutations: {
      retry: false,
    },
  },
};

/**
 * The module singleton. The plan rejects a Provider component, so the cache
 * lives here beside `src/stores/*` and every hook reads it through the
 * accessor below.
 */
const moduleClient = new QueryClient(queryClientOptions);

/** The client a test injected, or null when the app owns the cache. */
let injectedClient: QueryClient | null = null;

/** Answers the one client every hook and every SSE root must use. */
export function getQueryClient(): QueryClient {
  return injectedClient ?? moduleClient;
}

/**
 * The test seam. A test gives a fresh client so its cache starts empty, then
 * gives null to put the module singleton back. Production code never calls it.
 */
export function setQueryClientForTests(client: QueryClient | null): void {
  injectedClient = client;
}
