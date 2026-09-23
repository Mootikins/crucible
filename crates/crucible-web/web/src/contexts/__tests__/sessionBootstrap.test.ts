import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { bootstrapSessionWithFallback } from '../sessionBootstrap';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

// A pane that rebinds or unmounts aborts its bind. The session record can
// answer after that abort. The bind must then stop: a history read or an error
// log from a pane that is gone reaches a client and a `fetch` that belong to
// something else, and a log after the last test closes the worker mid-write.

const session = {
  session_id: 's-1', type: 'chat', title: 'T', state: 'active', kilns: ['k'], workspace: '/w',
  agent: { model: null }, started_at: '', event_count: 0, archived: false,
};

let env: TestQueryEnv;
let answerRecord: (answer: unknown) => void;

beforeEach(() => {
  // The test answers the record by hand, after it aborts the bind.
  env = createTestQueryEnv({
    'GET /api/session/s-1': () => new Promise((resolve) => { answerRecord = resolve; }),
  });
});

afterEach(() => {
  env.restore();
});

/** Starts a bind of `s-1`, aborts it while the record is in flight, and answers the record. */
async function abortThenAnswer(answer: unknown, readHistory: () => Promise<void> = async () => {}) {
  const controller = new AbortController();
  const loadHistory = vi.fn(readHistory);
  const bind = bootstrapSessionWithFallback({
    sessionId: 's-1',
    signal: controller.signal,
    setSessionTitle: () => {},
    loadHistory,
  });
  await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s-1')).toBe(1));
  controller.abort();
  answerRecord(answer);
  await bind;
  return loadHistory;
}

describe('bootstrapSessionWithFallback after its bind ends', () => {
  it('reads no history when the record answers', async () => {
    const loadHistory = await abortThenAnswer(session);
    expect(loadHistory).not.toHaveBeenCalled();
  });

  it('neither falls back nor logs when the record fails', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
    const failed = new Response(JSON.stringify(apiError(500, 'gone').body), { status: 500 });
    // The history read fails too, so a fallback that runs has an error to log.
    const loadHistory = await abortThenAnswer(failed, () => Promise.reject(new Error('no history')));
    expect(loadHistory).not.toHaveBeenCalled();
    expect(logged).not.toHaveBeenCalled();
    logged.mockRestore();
  });

  it('does not log a fallback read that fails after the bind ends', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
    const controller = new AbortController();
    let failHistory!: (err: Error) => void;
    const loadHistory = vi.fn(() => new Promise<void>((_, reject) => { failHistory = reject; }));
    const bind = bootstrapSessionWithFallback({
      sessionId: 's-1',
      signal: controller.signal,
      setSessionTitle: () => {},
      loadHistory,
    });
    await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s-1')).toBe(1));
    answerRecord(new Response(JSON.stringify(apiError(500, 'gone').body), { status: 500 }));
    await vi.waitFor(() => expect(loadHistory).toHaveBeenCalled());
    controller.abort();
    failHistory(new Error('no history'));
    await bind;
    expect(logged).not.toHaveBeenCalled();
    logged.mockRestore();
  });
});
