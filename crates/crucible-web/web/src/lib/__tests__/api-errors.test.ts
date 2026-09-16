import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { createMockFetch } from '@/test-utils';
import { createSession, listAllModels, listDir, listModes, sendChatMessage } from '../api';
import { notificationActions, notificationStore } from '@/stores/notificationStore';

// A daemon refusal reaches the browser as the daemon's own sentence — not a
// bare status. The user who created a session in its own folder saw "422" and
// nothing else, while the daemon had written exactly which root it refused.

const REFUSAL = "root is not a registered project or a session's own workspace folder";

/** The web error envelope every crucible-web route serialises. */
const refusal = (status: number, message: string) => ({
  status,
  body: { error: { code: status, message } },
});

const originalFetch = global.fetch;

const errorToasts = () =>
  notificationStore.notifications.filter((n) => n.type === 'error' && !n.dismissed);

beforeEach(() => {
  notificationActions.clearAll();
});

afterEach(() => {
  global.fetch = originalFetch;
});

describe('daemon refusals reach the user with the reason', () => {
  it('a refused folder listing raises one notification carrying the daemon sentence', async () => {
    global.fetch = createMockFetch({ 'GET /api/fs/list': refusal(422, REFUSAL) });

    await expect(listDir('/home/u/.crucible/workspaces/ses-1')).rejects.toThrow(REFUSAL);

    const toasts = errorToasts();
    expect(toasts).toHaveLength(1);
    expect(toasts[0].message).toContain(REFUSAL);
    // What was being attempted, too: the sentence alone names no folder.
    expect(toasts[0].message).toContain('/home/u/.crucible/workspaces/ses-1');
  });

  it('the same refusal repeated within a few seconds is one notification, not a column', async () => {
    global.fetch = createMockFetch({ 'GET /api/fs/list': refusal(422, REFUSAL) });

    await expect(listDir('/proj')).rejects.toThrow();
    await expect(listDir('/proj')).rejects.toThrow();
    await expect(listDir('/proj')).rejects.toThrow();

    expect(errorToasts()).toHaveLength(1);
  });

  it('a refused create rejects with the daemon reason for its caller to show', async () => {
    const reason =
      "workspace target 'worktree:feat/x' could not be resolved: no plugin provides workspace targets named 'worktree'";
    global.fetch = createMockFetch({ 'POST /api/session': refusal(422, reason) });

    await expect(createSession({ kilns: [], workspace_target: 'worktree:feat/x' })).rejects.toThrow(
      reason,
    );
  });

  it('a refused send, model list and mode list each notify with the reason', async () => {
    global.fetch = createMockFetch({
      'POST /api/chat/send': refusal(422, 'session ses-1 has no agent configured'),
      'GET /api/models': refusal(502, 'provider ollama is unreachable'),
      'GET /api/session/ses-1/modes': refusal(422, 'session ses-1 is not active'),
    });

    await expect(sendChatMessage('ses-1', 'hi')).rejects.toThrow('no agent configured');
    await expect(listAllModels()).rejects.toThrow('ollama is unreachable');
    await expect(listModes('ses-1')).rejects.toThrow('is not active');

    const messages = errorToasts().map((n) => n.message);
    expect(messages).toHaveLength(3);
    expect(messages.some((m) => m.includes('no agent configured'))).toBe(true);
    expect(messages.some((m) => m.includes('ollama is unreachable'))).toBe(true);
    expect(messages.some((m) => m.includes('is not active'))).toBe(true);
  });

  it('a body with no envelope still names the call and the status', async () => {
    global.fetch = createMockFetch({ 'GET /api/models': { status: 500 } });

    await expect(listAllModels()).rejects.toThrow('Failed to list models: HTTP 500');
    expect(errorToasts()[0].message).toBe('Failed to list models: HTTP 500');
  });
});
