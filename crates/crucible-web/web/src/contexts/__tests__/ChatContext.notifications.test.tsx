import { afterEach, expect, it } from 'vitest';
import { cleanup, render, waitFor } from '@solidjs/testing-library';
import { ChatProvider } from '../ChatContext';
import { resetTranscriptsForTests } from '../transcriptStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, FakeEventSource } from '@/test-utils/sse';
import { resetDaemonNotificationsForTests } from '@/lib/query/daemon-notification';
import { notificationActions, notificationStore } from '@/stores/notificationStore';

let env: TestQueryEnv | undefined;

afterEach(() => {
  cleanup();
  for (const notification of notificationStore.notifications) {
    if (notification.origin) notificationActions.dropOrigin(notification.origin.key);
  }
  notificationActions.clearAll();
  env?.restore();
  resetTranscriptsForTests();
  resetDaemonNotificationsForTests();
});

it('a delayed attach snapshot cannot resurrect a notice dismissed on the stream', async () => {
  installFakeEventSource();
  let release!: (value: unknown) => void;
  const pending = new Promise((resolve) => { release = resolve; });
  const notice = { id: 'dismissed-notice', kind: 'warning', message: 'already closed elsewhere' };
  const sentinel = { id: 'snapshot-sentinel', kind: 'warning', message: 'snapshot finished' };
  env = createTestQueryEnv({
    'GET /api/session/s1': () => ({
      session_id: 's1', type: 'chat', state: 'active', kilns: [], workspace: '/w',
      agent: { model: null }, started_at: '', event_count: 0, archived: false,
    }),
    'GET /api/session/s1/history': () => ({ session_id: 's1', history: [], total_events: 0 }),
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/session/s1/notifications': () => pending,
  });
  render(() => <ChatProvider sessionId="s1"><span>Chat</span></ChatProvider>);
  await waitFor(() => expect(FakeEventSource.instances.some((s) => s.url === '/api/chat/events/s1')).toBe(true));
  const sources = FakeEventSource.instances.filter((s) => s.url === '/api/chat/events/s1');
  expect(sources).toHaveLength(1);
  const source = sources[0]!;
  source.open();
  await waitFor(() => expect(env!.fetch.calls('GET /api/session/s1/notifications')).toBe(1));
  const visible = (id: string) => notificationStore.notifications.filter(
    (n) => !n.dismissed && n.origin?.key === id,
  );

  // Exercise the real ChatProvider/reducer, not a test-owned SSE callback.
  source.emit('notification_added', {
    event: 'notification_added', data: { notification: notice },
  });
  await waitFor(() => expect(visible(notice.id)).toHaveLength(1));
  source.emit('notification_dismissed', {
    event: 'notification_dismissed', data: { notification_id: notice.id },
  });
  await waitFor(() => expect(visible(notice.id)).toHaveLength(0));

  // Lists are newest first. The sentinel is rendered last after reversal,
  // proving the delayed snapshot has been consumed without a timing sleep.
  release({ notifications: [sentinel, notice] });
  await waitFor(() => expect(visible(sentinel.id)).toHaveLength(1));
  expect(visible(notice.id)).toHaveLength(0);
});
