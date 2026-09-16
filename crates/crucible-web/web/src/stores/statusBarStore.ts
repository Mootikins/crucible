import { createSignal } from 'solid-js';
import type { ChatMode, ContextUsage } from '@/lib/types';

// ── Global status bar state ──────────────────────────────────────────────
// StatusBar lives outside ChatProvider/SessionProvider, so it can't use
// context hooks. This module-level store is updated by ChatContext when
// events arrive, and read by StatusBar directly.
//
// It holds DISPLAY state only. `activeSessionId` names the session the
// focused pane shows, which is this client's own choice; the session's title
// and model are fields of the daemon's record, so a reader takes them from
// `useSession(id)` in `lib/query/sessions.ts` rather than from a second copy
// every pane had to remember to write.

const [chatMode, setChatMode] = createSignal<ChatMode>('ask');
const [contextUsage, setContextUsage] = createSignal<ContextUsage | null>(null);
const [notificationCount, setNotificationCount] = createSignal(0);
const [showThinking, setShowThinking] = createSignal(true);  // Toggle visibility of thinking blocks
const [activeSessionId, setActiveSessionId] = createSignal<string | null>(null);
// Session context shown in the shell header + status bar: the kiln the
// active session knows and the workspace it acts in. Falls back to the
// config-level kiln before any session is selected.
const [kilnPath, setKilnPath] = createSignal<string | null>(null);
const [workspacePath, setWorkspacePath] = createSignal<string | null>(null);

/** Last path segment, for displaying kiln/workspace paths as names. */
export function pathBasename(path: string | null): string | null {
  if (!path) return null;
  const segments = path.split('/').filter(Boolean);
  return segments.length ? segments[segments.length - 1] : null;
}

export const statusBarStore = {
  chatMode,
  contextUsage,
  notificationCount,
  showThinking,
  activeSessionId,
  kilnPath,
  workspacePath,
} as const;

export const statusBarActions = {
  setChatMode,
  setContextUsage,
  setNotificationCount,
  setShowThinking,
  setActiveSessionId,
  setKilnPath,
  setWorkspacePath,
} as const;
