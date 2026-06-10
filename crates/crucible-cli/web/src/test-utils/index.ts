/**
 * Test utilities and fixtures for Crucible web frontend tests.
 *
 * Exports:
 * - createMockFetch: Helper to create mock fetch functions for testing
 * - Fixtures: mockSession, mockProviders, mockModels, mockNotes, mockSearchResults
 */

export { createMockFetch, type MockFetchHandler } from './mock-fetch';
export {
  mockSession,
  mockProviders,
  mockModels,
  mockNotes,
  mockSearchResults,
} from './fixtures';
