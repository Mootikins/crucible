import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@solidjs/testing-library';
import { NotificationToast } from '../NotificationToast';
import { resetDaemonNotificationsForTests, showDaemonNotification } from '@/lib/query/daemon-notification';
import { notificationActions } from '@/stores/notificationStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

// `session.dismiss_notification` is an RPC method now
// ([[Simplification Plan#Step 19]] item 9); the browser calls
// `rpc('session.dismiss_notification', ...)` now.
const CLOSE = 'POST /api/rpc/session.dismiss_notification';

let env: TestQueryEnv;

beforeEach(() => {
  resetDaemonNotificationsForTests();
  notificationActions.clearAll();
  env = createTestQueryEnv({ [CLOSE]: { body: { success: true } } });
});

afterEach(() => {
  cleanup();
  env.restore();
});

describe('NotificationToast', () => {
  it('the close button of a daemon notice asks the daemon to close it for the session', async () => {
    showDaemonNotification({ id: 'n1', kind: 'warning', message: 'a notice for every session' }, 's1');
    render(() => <NotificationToast />);
    expect(screen.getByText('a notice for every session')).toBeTruthy();

    fireEvent.click(screen.getByLabelText('Dismiss notification'));

    await vi.waitFor(() => expect(env.fetch.calls(CLOSE)).toBe(1));
    expect(screen.queryByText('a notice for every session')).toBeNull();
  });
});
