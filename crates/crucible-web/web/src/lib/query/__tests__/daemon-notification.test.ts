import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  dropDaemonNotification,
  resetDaemonNotificationsForTests,
  showDaemonNotification,
} from '@/lib/query/daemon-notification';
import { notificationActions, notificationStore } from '@/stores/notificationStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

// `session.dismiss_notification` is an RPC method now
// ([[Simplification Plan#Step 19]] item 9): every session's close shares the
// one `POST /api/rpc/session.dismiss_notification` key, so telling two
// sessions' closes apart reads `session_id` off each call's own body.
const DISMISS = 'POST /api/rpc/session.dismiss_notification';

let env: TestQueryEnv;

beforeEach(() => {
  resetDaemonNotificationsForTests();
  notificationActions.clearAll();
  env = createTestQueryEnv({
    [DISMISS]: () => ({ success: true }),
  });
});

afterEach(() => {
  env.restore();
  vi.useRealTimers();
});

function open(message: string) {
  return notificationStore.notifications.filter((n) => !n.dismissed && n.message === message);
}

/** How many `session.dismiss_notification` calls named this session and notification. */
async function callsFor(sessionId: string, notificationId: string): Promise<number> {
  let matched = 0;
  for (let i = 0; i < env.fetch.calls(DISMISS); i += 1) {
    const sent = await env.fetch.sent(i);
    if (sent.path !== '/api/rpc/session.dismiss_notification') continue;
    const body = sent.body as { session_id?: string; notification_id?: string };
    if (body?.session_id === sessionId && body?.notification_id === notificationId) matched += 1;
  }
  return matched;
}

describe('daemon notifications', () => {
  it('a user close tells the daemon, once, for the session that showed it', async () => {
    showDaemonNotification({ id: 'n1', kind: 'warning', message: 'shared' }, 's1');
    const [entry] = open('shared');

    notificationActions.dismiss(entry.id);
    notificationActions.dismiss(entry.id);

    await vi.waitFor(async () => expect(await callsFor('s1', 'n1')).toBe(1));
    expect(open('shared')).toHaveLength(0);
  });

  it('a timeout takes the toast down and tells the daemon nothing', () => {
    vi.useFakeTimers();
    showDaemonNotification({ id: 'n1', kind: 'toast', message: 'shared' }, 's1');

    vi.advanceTimersByTime(60_000);

    expect(open('shared')).toHaveLength(0);
    expect(env.fetch.calls(DISMISS)).toBe(0);
  });

  it('one notice in two sessions is one toast, and a close reaches both sessions', async () => {
    showDaemonNotification({ id: 'n1', kind: 'toast', message: 'shared' }, 's1');
    showDaemonNotification({ id: 'n1', kind: 'toast', message: 'shared' }, 's2');
    expect(open('shared')).toHaveLength(1);

    notificationActions.clearAll();

    await vi.waitFor(async () => {
      expect(await callsFor('s1', 'n1')).toBe(1);
      expect(await callsFor('s2', 'n1')).toBe(1);
    });
  });

  it('a close in another client leaves the toast while a second session still shows it', () => {
    showDaemonNotification({ id: 'n1', kind: 'toast', message: 'shared' }, 's1');
    showDaemonNotification({ id: 'n1', kind: 'toast', message: 'shared' }, 's2');

    dropDaemonNotification('n1', 's1');
    expect(open('shared')).toHaveLength(1);

    dropDaemonNotification('n1', 's2');
    expect(open('shared')).toHaveLength(0);
    expect(env.fetch.calls(DISMISS)).toBe(0);
  });

  it('a failed close says so', async () => {
    env.restore();
    env = createTestQueryEnv({
      [DISMISS]: { status: 502, body: { error: { code: 502, message: 'daemon gone' } } },
    });
    showDaemonNotification({ id: 'n1', kind: 'toast', message: 'shared' }, 's1');

    notificationActions.dismiss(open('shared')[0].id);

    await vi.waitFor(() =>
      expect(
        notificationStore.notifications.some(
          (n) => !n.dismissed && n.message.startsWith('The daemon could not close the notification'),
        ),
      ).toBe(true),
    );
  });
});
