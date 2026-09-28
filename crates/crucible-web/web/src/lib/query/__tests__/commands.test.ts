import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { SessionCommand } from '@/lib/api';
import {
  fetchSlashCommandsOnce,
  resetCommandCache,
  useExecuteCommand,
  useSlashCommands,
} from '../commands';

const COMMANDS: SessionCommand[] = [
  { name: 'help', description: 'Show available commands', kind: 'builtin', command: 'help' },
  { name: 'reflect', description: 'Run a reflection pass', kind: 'plugin', plugin: 'alpha' },
];

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

describe('the slash command list', () => {
  // The catalog moves only when the daemon says so, so one GET per session
  // is the point.
  it('asks once however many times a composer reads it', async () => {
    env = createTestQueryEnv({ 'GET /api/session/s-1/commands': () => ({ commands: COMMANDS }) });

    expect(await fetchSlashCommandsOnce('s-1')).toEqual(COMMANDS);
    expect(await fetchSlashCommandsOnce('s-1')).toEqual(COMMANDS);
    const query = inRoot(() => useSlashCommands(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toEqual(COMMANDS));
    expect(env.fetch.calls('GET /api/session/s-1/commands')).toBe(1);
  });

  // A failed fetch must not poison the cache: the composer's next keystroke
  // has to be able to retry. The old module-level promise memo dropped itself
  // by hand for this, and the query holds no data to serve in its place.
  it('asks again after a refusal', async () => {
    let refuse = true;
    env = createTestQueryEnv({
      'GET /api/session/s-1/commands': () =>
        refuse
          ? new Response(JSON.stringify({ error: { code: 500, message: 'not ready' } }), {
              status: 500,
            })
          : { commands: COMMANDS },
    });

    await expect(fetchSlashCommandsOnce('s-1')).rejects.toThrow('Failed to list commands');
    refuse = false;

    expect(await fetchSlashCommandsOnce('s-1')).toEqual(COMMANDS);
    expect(env.fetch.calls('GET /api/session/s-1/commands')).toBe(2);
  });

  it('keeps one catalog per session', async () => {
    env = createTestQueryEnv({
      'GET /api/session/s-1/commands': () => ({ commands: COMMANDS }),
      'GET /api/session/s-2/commands': () => ({ commands: [] }),
    });

    expect(await fetchSlashCommandsOnce('s-1')).toEqual(COMMANDS);
    expect(await fetchSlashCommandsOnce('s-2')).toEqual([]);
  });

  it('asks again after the cache is reset', async () => {
    env = createTestQueryEnv({ 'GET /api/session/s-1/commands': () => ({ commands: COMMANDS }) });

    expect(await fetchSlashCommandsOnce('s-1')).toEqual(COMMANDS);
    await resetCommandCache();

    expect(await fetchSlashCommandsOnce('s-1')).toEqual(COMMANDS);
    expect(env.fetch.calls('GET /api/session/s-1/commands')).toBe(2);
  });
});

describe('useExecuteCommand', () => {
  it('posts the command to the session the caller named', async () => {
    let sent: unknown = null;
    env = createTestQueryEnv({
      'POST /api/session/s-1/command': async (request) => {
        sent = await request.json();
        return { result: 'switched', type: 'success' };
      },
    });

    const mutation = inRoot(() => useExecuteCommand(() => 's-1'));
    const answer = await mutation.mutateAsync('/model opus');

    expect(sent).toEqual({ command: '/model opus' });
    expect(answer).toEqual({ result: 'switched', type: 'success' });
  });

  // The composer prints the daemon's refusal in the transcript, so the
  // mutation has to reject rather than answer an empty result.
  it('rejects with the daemon sentence when the command is refused', async () => {
    env = createTestQueryEnv({
      'POST /api/session/s-1/command': apiError(400, 'no such command'),
    });

    const mutation = inRoot(() => useExecuteCommand(() => 's-1'));

    await expect(mutation.mutateAsync('/nope')).rejects.toThrow('Failed to execute command');
  });
});
