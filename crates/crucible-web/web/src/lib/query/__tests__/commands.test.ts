import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { SlashCommand } from '@/lib/api';
import {
  fetchSlashCommandsOnce,
  resetCommandCache,
  useExecuteCommand,
  useSlashCommands,
} from '../commands';

const COMMANDS: SlashCommand[] = [
  { name: 'help', args: '', description: 'Show available commands' },
  { name: 'model', args: '<name>', description: 'Switch to a different model' },
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
  // The daemon serves these from the constant `execute_command` dispatches on,
  // so they cannot change while it runs. One GET per browser is the point.
  it('asks once however many times a composer reads it', async () => {
    env = createTestQueryEnv({ 'GET /api/commands': () => ({ commands: COMMANDS }) });

    expect(await fetchSlashCommandsOnce()).toEqual(COMMANDS);
    expect(await fetchSlashCommandsOnce()).toEqual(COMMANDS);
    const query = inRoot(() => useSlashCommands());

    await vi.waitFor(() => expect(query.data).toEqual(COMMANDS));
    expect(env.fetch.calls('GET /api/commands')).toBe(1);
  });

  // A failed fetch must not poison the cache: the composer's next keystroke
  // has to be able to retry. The old module-level promise memo dropped itself
  // by hand for this, and the query holds no data to serve in its place.
  it('asks again after a refusal', async () => {
    let refuse = true;
    env = createTestQueryEnv({
      'GET /api/commands': () =>
        refuse
          ? new Response(JSON.stringify({ error: { code: 500, message: 'not ready' } }), {
              status: 500,
            })
          : { commands: COMMANDS },
    });

    await expect(fetchSlashCommandsOnce()).rejects.toThrow('Failed to list commands');
    refuse = false;

    expect(await fetchSlashCommandsOnce()).toEqual(COMMANDS);
    expect(env.fetch.calls('GET /api/commands')).toBe(2);
  });

  it('asks again after the cache is reset', async () => {
    env = createTestQueryEnv({ 'GET /api/commands': () => ({ commands: COMMANDS }) });

    expect(await fetchSlashCommandsOnce()).toEqual(COMMANDS);
    await resetCommandCache();

    expect(await fetchSlashCommandsOnce()).toEqual(COMMANDS);
    expect(env.fetch.calls('GET /api/commands')).toBe(2);
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
