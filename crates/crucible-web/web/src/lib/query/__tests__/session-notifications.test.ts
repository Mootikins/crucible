import { afterEach, expect, it, vi } from 'vitest';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, FakeEventSource } from '@/test-utils/sse';
import { notificationActions, notificationStore } from '@/stores/notificationStore';
import { resetDaemonNotificationsForTests } from '../daemon-notification';
import { sessionEvents, setEventRoute } from '../sse';

let env: TestQueryEnv;
afterEach(() => {
  env?.restore();
  resetDaemonNotificationsForTests();
  for (const n of notificationStore.notifications) {
    if (n.origin) notificationActions.dropOrigin(n.origin.key);
  }
  notificationActions.clearAll();
});
const notice = (id: string, message = id) => ({ id, kind: 'warning', message });
const visible = () => notificationStore.notifications.filter((n) => !n.dismissed).map((n) => n.message);
const pending = () => {
  let resolve!: (value: unknown) => void;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
};
function attach(session = 's1') {
  const stream = sessionEvents(session);
  const unsubscribe = stream.subscribe(() => {});
  const source = FakeEventSource.instances.at(-1)!;
  source.open();
  return { stream, source, unsubscribe };
}
function added(source: FakeEventSource, id: string, message = id) {
  source.emit('notification_added', { event: 'notification_added', data: { notification: notice(id, message) } });
}
function dismissed(source: FakeEventSource, id: string) {
  source.emit('notification_dismissed', { event: 'notification_dismissed', data: { notification_id: id } });
}

it('two panes share one snapshot and newer additions override its old text', async () => {
  installFakeEventSource();
  const read = pending();
  env = createTestQueryEnv({ 'GET /api/session/s1/notifications': () => read.promise });
  const { source } = attach();
  sessionEvents('s1').subscribe(() => {});
  expect(FakeEventSource.instances).toHaveLength(1);
  await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s1/notifications')).toBe(1));
  added(source, 'n1', 'new text');
  added(source, 'n2', 'only live');
  read.resolve({ notifications: [notice('done'), notice('n1', 'old text')] });
  await vi.waitFor(() => expect(visible()).toContain('done'));
  expect(visible()).toEqual(expect.arrayContaining(['new text', 'only live']));
  expect(visible()).not.toContain('old text');
});

it('a reconnect discards the old response and removes missed dismissals', async () => {
  installFakeEventSource();
  const old = pending();
  const next = pending();
  let reads = 0;
  env = createTestQueryEnv({ 'GET /api/session/s1/notifications': () => ++reads === 1 ? old.promise : next.promise });
  const { source } = attach();
  await vi.waitFor(() => expect(reads).toBe(1));
  added(source, 'gone');
  // The transport reports connected on every open, including automatic retries.
  source.open();
  await vi.waitFor(() => expect(reads).toBe(2));
  next.resolve({ notifications: [notice('current')] });
  await vi.waitFor(() => expect(visible()).toContain('current'));
  expect(visible()).not.toContain('gone');
  old.resolve({ notifications: [notice('stale')] });
  await old.promise;
  await new Promise((r) => setTimeout(r, 0));
  expect(visible()).not.toContain('stale');
});

it('detach invalidates a pending snapshot, and a gap starts a fresh one', async () => {
  installFakeEventSource();
  const read = pending();
  env = createTestQueryEnv({ 'GET /api/session/s1/notifications': () => read.promise });
  const { source, unsubscribe } = attach();
  await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s1/notifications')).toBe(1));
  source.emit('stream_gap', { event: 'stream_gap', data: { dropped: 1 } });
  await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s1/notifications')).toBe(2));
  unsubscribe();
  await vi.waitFor(() => expect(source.closed).toBe(true));
  read.resolve({ notifications: [notice('detached')] });
  await read.promise;
  await new Promise((r) => setTimeout(r, 0));
  expect(visible()).not.toContain('detached');
});

it('a dismissal in one session leaves the other session’s shared notice visible', async () => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'GET /api/session/s1/notifications': { body: { notifications: [notice('shared')] } },
    'GET /api/session/s2/notifications': { body: { notifications: [notice('shared')] } },
  });
  const one = attach('s1');
  const two = attach('s2');
  await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s2/notifications')).toBe(1));
  await vi.waitFor(() => expect(visible()).toContain('shared'));
  added(two.source, 'shared');
  dismissed(one.source, 'shared');
  expect(visible()).toContain('shared');
  dismissed(two.source, 'shared');
  expect(visible()).not.toContain('shared');
});

it('a failed snapshot reports the error and live warning/toast events still work', async () => {
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/session/s1/notifications': { status: 502, body: { error: { code: 502, message: 'offline' } } } });
  const { source } = attach();
  await vi.waitFor(() => expect(visible().some((s) => s.includes('not available'))).toBe(true));
  added(source, 'warning');
  source.emit('notification_added', { event: 'notification_added', data: { notification: { id: 'info', kind: 'toast', message: 'saved' } } });
  expect(notificationStore.notifications.find((n) => n.message === 'warning')?.type).toBe('warning');
  expect(notificationStore.notifications.find((n) => n.message === 'saved')?.type).toBe('info');
  dismissed(source, 'warning');
  expect(visible()).not.toContain('warning');
});

it('a notification that its timer hid does not show again on a reconnect or a gap', async () => {
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/session/s1/notifications': { body: { notifications: [notice('old')] } } });
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  try {
    const { source } = attach();
    await vi.waitFor(() => expect(visible()).toContain('old'));
    // The toast's own timer takes it down; the daemon still holds it open.
    vi.advanceTimersByTime(60_000);
    expect(visible()).not.toContain('old');

    source.open();
    await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s1/notifications')).toBe(2));
    source.emit('stream_gap', { event: 'stream_gap', data: { dropped: 1 } });
    source.open();
    await vi.waitFor(() => expect(env.fetch.calls('GET /api/session/s1/notifications')).toBe(4));
    vi.useRealTimers();
    // The last snapshot resolves after its fetch; let it apply.
    await new Promise((r) => setTimeout(r, 10));

    expect(visible()).not.toContain('old');
    expect(notificationStore.notifications.filter((n) => n.message === 'old')).toHaveLength(1);
  } finally {
    vi.useRealTimers();
  }
});

it('one open of the chat stream runs its reconcile once', () => {
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/session/s1/notifications': { body: { notifications: [] } } });
  const reconcile = vi.fn();
  setEventRoute('session', null, reconcile);
  const opened = vi.fn();
  sessionEvents('s1').subscribe(() => {}, opened);
  const source = FakeEventSource.instances.at(-1)!;
  source.open();
  expect(reconcile).toHaveBeenCalledTimes(1);
  expect(opened).toHaveBeenCalledTimes(1);
  source.open();
  expect(reconcile).toHaveBeenCalledTimes(2);
  expect(opened).toHaveBeenCalledTimes(1);
});
