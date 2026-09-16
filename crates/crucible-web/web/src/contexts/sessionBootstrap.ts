import { fetchSessionOnce } from '@/lib/query/sessions';
import { sessionDefaultKiln } from '@/lib/session-scope';
import { kilnPathOf } from '@/stores/kilnStore';
import type { ChatMode } from '@/lib/types';
import { statusBarActions } from '@/stores/statusBarStore';

interface BootstrapSessionParams {
  sessionId: string;
  signal: AbortSignal;
  setSessionTitle: (title: string | null) => void;
  /** Hydrate the persisted session mode into the chat UI. */
  setChatMode?: (mode: ChatMode) => void;
  loadHistory: (sessionId: string, signal?: AbortSignal) => Promise<void>;
}

/** Restore the mode the daemon persisted for this session.
 *
 * Deliberately does not check the id against a known set: modes are declared
 * in Lua, so a restored `review` session was being silently dropped here and
 * shown as Normal while the agent ran review. The daemon is the authority on
 * whether the mode is valid — it validates on `set_mode` and falls back if the
 * declaration is gone. */
function hydrateMode(mode: string | null, setChatMode?: (mode: ChatMode) => void): void {
  if (!setChatMode || !mode) return;
  setChatMode(mode);
}

/**
 * Announce which session this pane shows.
 *
 * The title and the model are NOT written here any more. They are fields of
 * the session record, and the record has one owner: the `['session', id]`
 * query this function's caller just read. The status bar reads that key, so a
 * rename reaches it through the cache rather than through a second copy this
 * pane pushed at it.
 */
function announceSession(sessionId: string) {
  statusBarActions.setActiveSessionId(sessionId);
}

export async function bootstrapSessionWithFallback({
  sessionId,
  signal,
  setSessionTitle,
  setChatMode,
  loadHistory,
}: BootstrapSessionParams): Promise<void> {
  try {
    const session = await fetchSessionOnce(sessionId);
    setSessionTitle(session.title ?? null);
    // The daemon persists the session mode on the agent config; without this
    // a page reload silently shows "Normal" while the agent stays in plan.
    // `session.get` nests it under `agent`, which is the only route that
    // sends it at all — `session.list` carries no mode.
    hydrateMode(session.agent?.mode ?? null, setChatMode);
    announceSession(session.session_id);
    // The status bar shows a path, the session stores a name.
    statusBarActions.setKilnPath(kilnPathOf(sessionDefaultKiln(session)));
    statusBarActions.setWorkspacePath(session.workspace || null);
    await loadHistory(session.session_id, signal);
    return;
  } catch (err) {
    if (err instanceof Error && err.name === 'AbortError') {
      return;
    }
  }

  // `session.get` failed, so the daemon has no live row — but history now
  // loads from the daemon's own session store rather than a kiln directory,
  // so there is nothing left to look up before asking for it.
  try {
    setSessionTitle(null);
    announceSession(sessionId);
    await loadHistory(sessionId, signal);
  } catch (fallbackErr) {
    if (fallbackErr instanceof Error && fallbackErr.name === 'AbortError') {
      return;
    }
    console.error('Failed to load session metadata:', fallbackErr);
  }
}
