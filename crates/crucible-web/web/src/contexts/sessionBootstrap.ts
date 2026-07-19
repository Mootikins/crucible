import { getConfig, getSession, listSessions } from '@/lib/api';
import { statusBarActions } from '@/stores/statusBarStore';

interface BootstrapSessionParams {
  sessionId: string;
  signal: AbortSignal;
  setSessionTitle: (title: string | null) => void;
  /** Hydrate the persisted session mode (normal/plan/auto) into the chat UI. */
  setChatMode?: (mode: 'normal' | 'plan' | 'auto') => void;
  loadHistory: (sessionId: string, kiln: string, signal?: AbortSignal) => Promise<void>;
}

function hydrateMode(
  mode: string | null,
  setChatMode?: (mode: 'normal' | 'plan' | 'auto') => void
): void {
  if (!setChatMode) return;
  if (mode === 'normal' || mode === 'plan' || mode === 'auto') {
    setChatMode(mode);
  }
}

function syncPrimaryStatus(sessionId: string, title: string | null, model: string | null) {
  statusBarActions.setActiveModel(model ?? null);
  statusBarActions.setActiveSessionId(sessionId);
  statusBarActions.setActiveSessionTitle(title);
}

function syncFallbackStatus(sessionId: string, title: string | null, model: string | null) {
  statusBarActions.setActiveModel(model ?? null);
  statusBarActions.setActiveSessionId(sessionId);
  statusBarActions.setActiveSessionTitle(title ?? `Session ${sessionId.slice(0, 8)}`);
}

export async function bootstrapSessionWithFallback({
  sessionId,
  signal,
  setSessionTitle,
  setChatMode,
  loadHistory,
}: BootstrapSessionParams): Promise<void> {
  try {
    const session = await getSession(sessionId);
    setSessionTitle(session.title);
    // The daemon persists the session mode on the agent config; without this
    // a page reload silently shows "Normal" while the agent stays in plan.
    hydrateMode(session.agent_mode, setChatMode);
    syncPrimaryStatus(session.id, session.title, session.agent_model ?? null);
    statusBarActions.setKilnPath(session.kiln || null);
    statusBarActions.setWorkspacePath(session.workspace || null);
    await loadHistory(session.id, session.kiln, signal);
    return;
  } catch (err) {
    if (err instanceof Error && err.name === 'AbortError') {
      return;
    }
  }

  try {
    const config = await getConfig();
    const sessions = await listSessions({ kiln: config.kiln_path });
    const persistedSession = sessions.find((session) => session.id === sessionId) ?? null;
    const sessionKiln = persistedSession?.kiln || config.kiln_path;

    setSessionTitle(persistedSession?.title ?? null);
    syncFallbackStatus(sessionId, persistedSession?.title ?? null, persistedSession?.agent_model ?? null);
    await loadHistory(sessionId, sessionKiln, signal);
  } catch (fallbackErr) {
    if (fallbackErr instanceof Error && fallbackErr.name === 'AbortError') {
      return;
    }
    console.error('Failed to load session metadata:', fallbackErr);
  }
}
