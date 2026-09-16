/**
 * Test utilities and fixtures for Crucible web frontend tests.
 *
 * Exports:
 * - createMockFetch, apiError: mock `fetch` functions and the error envelope
 * - withQueryClient, createTestQueryEnv, createTestQueryClient: the query seam
 * - FakeEventSource, installFakeEventSource, onlyEventSource: the SSE seam
 * - Fixtures: mockSession, mockProviders, mockModels, mockNotes, mockSearchResults
 */

export {
  createMockFetch,
  apiError,
  type MockFetch,
  type MockFetchAnswer,
  type MockFetchHandler,
  type MockFetchRoute,
} from './mock-fetch';
export {
  withQueryClient,
  createTestQueryClient,
  createTestQueryEnv,
  type TestQueryEnv,
} from './query';
export { FakeEventSource, installFakeEventSource, onlyEventSource } from './sse';
export {
  mockSession,
  mockProviders,
  mockModels,
  mockNotes,
  mockSearchResults,
} from './fixtures';
