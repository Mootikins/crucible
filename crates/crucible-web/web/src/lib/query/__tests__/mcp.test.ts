import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { useMcpStatus } from '../mcp';

const STATUS = { running: true, port: 3847 };

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useMcpStatus', () => {
  it('asks once for two readers of the settings pane', async () => {
    env = createTestQueryEnv({ 'GET /api/mcp/status': () => STATUS });

    const both = inRoot(() => ({ first: useMcpStatus(), second: useMcpStatus() }));

    await vi.waitFor(() => expect(both.first.data).toEqual(STATUS));
    expect(both.second.data).toEqual(STATUS);
    expect(env.fetch.calls('GET /api/mcp/status')).toBe(1);
  });

  // The pane draws a refusal and a retry button, so the error has to reach it
  // rather than turn into an empty table of rows.
  it('holds the refusal for the pane to draw', async () => {
    env = createTestQueryEnv({
      'GET /api/mcp/status': apiError(503, 'the MCP server is not running'),
    });

    const query = inRoot(() => useMcpStatus());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to get MCP status');
    expect(query.data).toBeUndefined();
  });
});
