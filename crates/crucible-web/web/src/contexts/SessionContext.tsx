import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
  Accessor,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { Session, CreateSessionParams, ProviderInfo } from '@/lib/types';
import {
  createSession as apiCreateSession,
  listSessions as apiListSessions,
  getSession as apiGetSession,
  getSessionHistory as apiGetSessionHistory,
  pauseSession as apiPauseSession,
  resumeSession as apiResumeSession,
  endSession as apiEndSession,
  cancelSession as apiCancelSession,
  deleteSession as apiDeleteSession,
  archiveSession as apiArchiveSession,
  unarchiveSession as apiUnarchiveSession,
  listModels as apiListModels,
  switchModel as apiSwitchModel,
  setSessionTitle as apiSetSessionTitle,
  listProviders as apiListProviders,
} from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { findTabBySessionId } from '@/lib/session-actions';
import { windowActions } from '@/stores/windowStore';

export interface SessionContextValue {
  currentSession: Accessor<Session | null>;
  sessions: Accessor<Session[]>;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
  availableModels: Accessor<string[]>;
  providers: Accessor<ProviderInfo[]>;
  selectedProvider: Accessor<ProviderInfo | null>;
  createSession: (params: CreateSessionParams) => Promise<Session>;
  selectSession: (id: string) => Promise<void>;
  refreshSessions: (filters?: { kiln?: string; workspace?: string; includeArchived?: boolean }) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  endSession: () => Promise<void>;
  cancelCurrentOperation: () => Promise<boolean>;
  switchModel: (modelId: string) => Promise<void>;
  refreshModels: () => Promise<void>;
  setSessionTitle: (title: string) => Promise<void>;
  refreshProviders: () => Promise<void>;
  selectProvider: (providerType: string) => void;
  deleteSession: (sessionId: string) => Promise<void>;
  archiveSession: (sessionId: string) => Promise<void>;
  unarchiveSession: (sessionId: string) => Promise<void>;
}

interface SessionProviderProps {
  initialKiln?: string;
  initialWorkspace?: string;
  children: any;
}

const SessionContext = createContext<SessionContextValue>();

export const SessionProvider: ParentComponent<SessionProviderProps> = (props) => {
  const [currentSession, setCurrentSession] = createSignal<Session | null>(null);
  const [sessions, setSessions] = createStore<Session[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [availableModels, setAvailableModels] = createSignal<string[]>([]);
  const [providers, setProviders] = createSignal<ProviderInfo[]>([]);
  const [selectedProvider, setSelectedProvider] = createSignal<ProviderInfo | null>(null);

  const hydrateSession = async (sessionId: string, kiln: string | undefined): Promise<boolean> => {
    if (!kiln) return false;
    try {
      await apiGetSessionHistory(sessionId, kiln, 1, 0);
      return true;
    } catch {
      return false;
    }
  };

  const refreshSessions = async (filters?: { kiln?: string; workspace?: string; includeArchived?: boolean }) => {
    setIsLoading(true);
    setError(null);
    
    try {
      const list = await apiListSessions({
        kiln: filters?.kiln ?? props.initialKiln,
        workspace: filters?.workspace ?? props.initialWorkspace,
        includeArchived: filters?.includeArchived ?? false,
      });
      setSessions(list);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load sessions';
      setError(msg);
      notificationActions.addNotification('error', msg);
      console.error('Failed to refresh sessions:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const patchSessionById = (
    sessionId: string,
    patch: Partial<Session> | ((session: Session) => Session),
  ): Session | null => {
    const applyPatch = (session: Session): Session => (
      typeof patch === 'function'
        ? patch(session)
        : { ...session, ...patch }
    );

    let updatedSession: Session | null = null;
    const current = currentSession();
    if (current?.id === sessionId) {
      updatedSession = applyPatch(current);
      setCurrentSession(updatedSession);
    }

    setSessions(produce((list) => {
      const idx = list.findIndex((s) => s.id === sessionId);
      if (idx === -1) return;
      updatedSession = applyPatch(list[idx]);
      list[idx] = updatedSession;
    }));

    return updatedSession;
  };

  const withSessionAction = async <T,>(
    action: () => Promise<T>,
    options: {
      errorMessage: string;
      successMessage?: string;
      rethrow?: boolean;
      logPrefix: string;
    },
  ): Promise<T | undefined> => {
    setError(null);

    try {
      const result = await action();
      if (options.successMessage) {
        notificationActions.addNotification('success', options.successMessage);
      }
      return result;
    } catch (err) {
      const msg = err instanceof Error ? err.message : options.errorMessage;
      setError(msg);
      notificationActions.addNotification('error', msg);
      console.error(`${options.logPrefix}:`, err);
      if (options.rethrow) {
        throw err;
      }
      return undefined;
    }
  };

  const createSession = async (params: CreateSessionParams): Promise<Session> => {
    setIsLoading(true);
    try {
      const session = await withSessionAction(async () => {
        const created = await apiCreateSession(params);
        setSessions(produce((s) => s.unshift(created)));
        setCurrentSession(created);
        window.dispatchEvent(new CustomEvent('crucible:open-session', {
          detail: { sessionId: created.id, title: created.title || 'New Session' },
        }));
        await refreshModels(created);
        return created;
      }, {
        errorMessage: 'Failed to create session',
        successMessage: 'Session created',
        rethrow: true,
        logPrefix: 'Failed to create session',
      });
      if (!session) {
        throw new Error('Failed to create session');
      }
      return session;
    } finally {
      setIsLoading(false);
    }
  };

  const selectSession = async (id: string) => {
    const existing = sessions.find((s) => s.id === id);
    if (existing) {
      if (!(await apiGetSession(existing.id).then(() => true).catch(() => false))) {
        await hydrateSession(existing.id, existing.kiln);
      }

      setCurrentSession(existing);
      // Auto-resume paused sessions
      if (existing.state === 'paused') {
        try {
          await apiResumeSession(id);
          updateCurrentSessionState('active');
        } catch (err) {
          console.error('Failed to resume session:', err);
          // Continue to open session even if resume fails (graceful degradation)
        }
      }
      window.dispatchEvent(new CustomEvent('crucible:open-session', {
        detail: { sessionId: id, title: existing.title || `Session ${id.slice(0, 8)}` },
      }));
      await refreshModels(existing);
      return;
    }

    setIsLoading(true);
    setError(null);
    
    try {
      const session = await apiGetSession(id).catch(async () => {
        const hydrated = await hydrateSession(id, props.initialKiln);
        if (!hydrated) {
          throw new Error('Failed to load session');
        }
        return await apiGetSession(id);
      });
      setCurrentSession(session);
      // Auto-resume paused sessions
      if (session.state === 'paused') {
        try {
          await apiResumeSession(id);
          updateCurrentSessionState('active');
        } catch (err) {
          console.error('Failed to resume session:', err);
          // Continue to open session even if resume fails (graceful degradation)
        }
      }
      window.dispatchEvent(new CustomEvent('crucible:open-session', {
        detail: { sessionId: id, title: session.title || `Session ${id.slice(0, 8)}` },
      }));
      await refreshModels(session);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load session';
      setError(msg);
      console.error('Failed to select session:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const updateCurrentSessionState = (state: Session['state']) => {
    const session = currentSession();
    if (!session) return;

    patchSessionById(session.id, { state });
  };

  const pauseSession = async () => {
    const session = currentSession();
    if (!session) return;

    await withSessionAction(async () => {
      await apiPauseSession(session.id);
      updateCurrentSessionState('paused');
    }, {
      errorMessage: 'Failed to pause session',
      logPrefix: 'Failed to pause session',
    });
  };

  const resumeSession = async () => {
    const session = currentSession();
    if (!session) return;

    await withSessionAction(async () => {
      await apiResumeSession(session.id);
      updateCurrentSessionState('active');
    }, {
      errorMessage: 'Failed to resume session',
      logPrefix: 'Failed to resume session',
    });
  };

  const endSession = async () => {
    const session = currentSession();
    if (!session) return;

    await withSessionAction(async () => {
      await apiEndSession(session.id);
      updateCurrentSessionState('ended');
      setCurrentSession(null);
    }, {
      errorMessage: 'Failed to end session',
      logPrefix: 'Failed to end session',
    });
  };

  const deleteSession = async (sessionId: string) => {
    if (!confirm('Delete this session? This cannot be undone.')) return;

    await withSessionAction(async () => {
      await apiDeleteSession(sessionId);
      // Remove from local store for snappy UX
      setSessions(produce((list) => {
        const idx = list.findIndex((s) => s.id === sessionId);
        if (idx !== -1) list.splice(idx, 1);
      }));
      // Close open chat tab if any
      const openTab = findTabBySessionId(sessionId);
      if (openTab) {
        windowActions.removeTab(openTab.groupId, openTab.tab.id);
      }
      // Clear current session if it was the deleted one
      if (currentSession()?.id === sessionId) {
        setCurrentSession(null);
      }
    }, {
      errorMessage: 'Failed to delete session',
      successMessage: 'Session deleted',
      logPrefix: 'Failed to delete session',
    });
  };

  const archiveSession = async (sessionId: string) => {
    await withSessionAction(async () => {
      await apiArchiveSession(sessionId);
      // Remove from local store (archived sessions are hidden from default listing)
      setSessions(produce((list) => {
        const idx = list.findIndex((s) => s.id === sessionId);
        if (idx !== -1) list.splice(idx, 1);
      }));
      // Close open chat tab if any
      const openTab = findTabBySessionId(sessionId);
      if (openTab) {
        windowActions.removeTab(openTab.groupId, openTab.tab.id);
      }
      // Clear current session if it was the archived one
      if (currentSession()?.id === sessionId) {
        setCurrentSession(null);
      }
    }, {
      errorMessage: 'Failed to archive session',
      successMessage: 'Session archived',
      logPrefix: 'Failed to archive session',
    });
  };

  const unarchiveSession = async (sessionId: string) => {
    await withSessionAction(async () => {
      await apiUnarchiveSession(sessionId);
      await refreshSessions();
    }, {
      errorMessage: 'Failed to unarchive session',
      successMessage: 'Session unarchived',
      logPrefix: 'Failed to unarchive session',
    });
  };

  const cancelCurrentOperation = async (): Promise<boolean> => {
    const session = currentSession();
    if (!session) return false;

    try {
      return await apiCancelSession(session.id);
    } catch (err) {
      console.error('Failed to cancel operation:', err);
      return false;
    }
  };

  const refreshModels = async (sessionOverride?: Session) => {
    const session = sessionOverride ?? currentSession();
    if (!session?.id) {
      setAvailableModels([]);
      return;
    }

    try {
      const models = await apiListModels(session.id);
      if (models.length > 0) {
        setAvailableModels(models);
        return;
      }
      // Fall back to provider models if session has no agent configured
      const provider = selectedProvider() ?? providers()[0];
      if (provider?.models) {
        setAvailableModels(provider.models);
      } else {
        setAvailableModels([]);
      }
    } catch (err) {
      const msg = 'Failed to load models';
      notificationActions.addNotification('error', msg);
      console.error(msg, err);
      // Fall back to provider models on error
      const provider = selectedProvider() ?? providers()[0];
      if (provider?.models) {
        setAvailableModels(provider.models);
      } else {
        setAvailableModels([]);
      }
    }
  };

  const switchModel = async (modelId: string) => {
    const session = currentSession();
    if (!session) return;

    await withSessionAction(async () => {
      await apiSwitchModel(session.id, modelId);
      patchSessionById(session.id, { agent_model: modelId });
    }, {
      errorMessage: 'Failed to switch model',
      logPrefix: 'Failed to switch model',
    });
  };

  const setSessionTitle = async (title: string) => {
    const session = currentSession();
    if (!session) return;

    await withSessionAction(async () => {
      await apiSetSessionTitle(session.id, title);
      patchSessionById(session.id, { title });
    }, {
      errorMessage: 'Failed to set session title',
      logPrefix: 'Failed to set session title',
    });
  };

  const refreshProviders = async () => {
    try {
      const providerList = await apiListProviders();
      setProviders(providerList);
      if (providerList.length > 0 && !selectedProvider()) {
        setSelectedProvider(providerList[0]);
      }
    } catch (err) {
      console.error('Failed to load providers:', err);
    }
  };

  const selectProvider = (providerType: string) => {
    const provider = providers().find((p) => p.provider_type === providerType);
    if (provider) {
      setSelectedProvider(provider);
    }
  };

  createEffect(() => {
    if (!props.initialKiln) return; // Guard: skip when kiln not yet available
    refreshSessions({
      kiln: props.initialKiln,
      workspace: props.initialWorkspace,
    });
    refreshProviders();
  });

  const value: SessionContextValue = {
    currentSession,
    sessions: () => sessions,
    isLoading,
    error,
    availableModels,
    providers,
    selectedProvider,
    createSession,
    selectSession,
    refreshSessions,
    pauseSession,
    resumeSession,
    endSession,
    cancelCurrentOperation,
    switchModel,
    refreshModels,
    setSessionTitle,
    refreshProviders,
    selectProvider,
    deleteSession,
    archiveSession,
    unarchiveSession,
  };

  return (
    <SessionContext.Provider value={value}>
      {props.children}
    </SessionContext.Provider>
  );
};

export function useSession(): SessionContextValue {
  const context = useContext(SessionContext);
  if (!context) {
    throw new Error('useSession must be used within a SessionProvider');
  }
  return context;
}

const noopAsync = async () => {};

const fallbackSessionContext: SessionContextValue = {
  currentSession: () => null,
  sessions: () => [],
  isLoading: () => false,
  error: () => null,
  availableModels: () => [],
  providers: () => [],
  selectedProvider: () => null,
  createSession: () => Promise.reject(new Error('No session context')),
  selectSession: noopAsync,
  refreshSessions: noopAsync,
  pauseSession: noopAsync,
  resumeSession: noopAsync,
  endSession: noopAsync,
  cancelCurrentOperation: () => Promise.resolve(false),
  switchModel: noopAsync,
  refreshModels: noopAsync,
  setSessionTitle: noopAsync,
  refreshProviders: noopAsync,
  selectProvider: () => {},
  deleteSession: noopAsync,
  archiveSession: noopAsync,
  unarchiveSession: noopAsync,
};

export function useSessionSafe(): SessionContextValue {
  const context = useContext(SessionContext);
  return context ?? fallbackSessionContext;
}
