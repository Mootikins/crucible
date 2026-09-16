import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { AgentProfileEntry } from '@/lib/types';
import { useAgents } from '../agents';

/** The storage key `swrLocal('agents')` wrote, which the hook keeps. */
const STORAGE_KEY = 'crucible:cache:agents';

const PROBED: AgentProfileEntry[] = [
  {
    name: 'claude',
    description: 'Claude Code via ACP',
    command: 'npx',
    is_builtin: true,
    available: true,
  },
];
const STORED: AgentProfileEntry[] = [
  {
    name: 'stored',
    description: 'from the last run',
    command: 'stored',
    is_builtin: false,
    available: true,
  },
];

/** The envelope `GET /api/agents` answers; `listAgents` unwraps `agents`. */
function agentsBody(agents: AgentProfileEntry[]): { agents: AgentProfileEntry[] } {
  return { agents };
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

describe('useAgents', () => {
  it('fetches once for the desktop composer and the phone sheet together', async () => {
    env = createTestQueryEnv({ 'GET /api/agents': () => agentsBody(PROBED) });

    const both = inRoot(() => ({ first: useAgents(), second: useAgents() }));

    await vi.waitFor(() => expect(both.first.data).toEqual(PROBED));
    expect(both.second.data).toEqual(PROBED);
    expect(env.fetch.calls('GET /api/agents')).toBe(1);
  });

  it('paints the stored roster before the probe answers', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(STORED));
    let release: (() => void) | undefined;
    const answered = new Promise<void>((resolve) => {
      release = resolve;
    });
    env = createTestQueryEnv({
      'GET /api/agents': async () => {
        await answered;
        return agentsBody(PROBED);
      },
    });

    const query = inRoot(() => useAgents());

    // Probing an agent runs its command, so the answer is slow. The first read
    // already has the last roster rather than an empty chip menu.
    expect(query.data).toEqual(STORED);

    release?.();
    await vi.waitFor(() => expect(query.data).toEqual(PROBED));
    expect(env.fetch.calls('GET /api/agents')).toBe(1);
  });

  it('writes the probed roster back to storage', async () => {
    env = createTestQueryEnv({ 'GET /api/agents': () => agentsBody(PROBED) });

    const query = inRoot(() => useAgents());

    await vi.waitFor(() => expect(query.data).toEqual(PROBED));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(PROBED);
  });

  it('surfaces a refusal as an error rather than as an empty roster', async () => {
    env = createTestQueryEnv({
      'GET /api/agents': apiError(500, 'the agent registry is closed'),
    });

    const query = inRoot(() => useAgents());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to list agents');
    expect(query.data).toBeUndefined();
  });
});
