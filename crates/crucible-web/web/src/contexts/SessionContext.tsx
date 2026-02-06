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
  pauseSession as apiPauseSession,
  resumeSession as apiResumeSession,
  endSession as apiEndSession,
  cancelSession as apiCancelSession,
  listModels as apiListModels,
  switchModel as apiSwitchModel,
  setSessionTitle as apiSetSessionTitle,
  listProviders as apiListProviders,
} from '@/lib/api';

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
  refreshSessions: (filters?: { kiln?: string; workspace?: string }) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  endSession: () => Promise<void>;
  cancelCurrentOperation: () => Promise<boolean>;
  switchModel: (modelId: string) => Promise<void>;
  refreshModels: () => Promise<void>;
  setSessionTitle: (title: string) => Promise<void>;
  refreshProviders: () => Promise<void>;
  selectProvider: (providerType: string) => void;
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

  const refreshSessions = async (filters?: { kiln?: string; workspace?: string }) => {
    setIsLoading(true);
    setError(null);
    
    try {
      const list = await apiListSessions({
        kiln: filters?.kiln ?? props.initialKiln,
        workspace: filters?.workspace ?? props.initialWorkspace,
      });
      setSessions(list);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load sessions';
      setError(msg);
      console.error('Failed to refresh sessions:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const createSession = async (params: CreateSessionParams): Promise<Session> => {
    setIsLoading(true);
    setError(null);
    
    try {
      const session = await apiCreateSession(params);
      setSessions(produce((s) => s.unshift(session)));
      setCurrentSession(session);
      await refreshModels(session);
      return session;
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to create session';
      setError(msg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  };

  const selectSession = async (id: string) => {
    const existing = sessions.find((s) => s.id === id);
    if (existing) {
      setCurrentSession(existing);
      await refreshModels(existing);
      return;
    }

    setIsLoading(true);
    setError(null);
    
    try {
      const session = await apiGetSession(id);
      setCurrentSession(session);
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
    
    const updated = { ...session, state };
    setCurrentSession(updated);
    
    setSessions(produce((list) => {
      const idx = list.findIndex((s) => s.id === session.id);
      if (idx !== -1) {
        list[idx] = updated;
      }
    }));
  };

  const pauseSession = async () => {
    const session = currentSession();
    if (!session) return;

    setError(null);
    try {
      await apiPauseSession(session.id);
      updateCurrentSessionState('paused');
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to pause session';
      setError(msg);
      console.error('Failed to pause session:', err);
    }
  };

  const resumeSession = async () => {
    const session = currentSession();
    if (!session) return;

    setError(null);
    try {
      await apiResumeSession(session.id);
      updateCurrentSessionState('active');
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to resume session';
      setError(msg);
      console.error('Failed to resume session:', err);
    }
  };

  const endSession = async () => {
    const session = currentSession();
    if (!session) return;

    setError(null);
    try {
      await apiEndSession(session.id);
      updateCurrentSessionState('ended');
      setCurrentSession(null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to end session';
      setError(msg);
      console.error('Failed to end session:', err);
    }
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
      console.error('Failed to load models:', err);
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

    setError(null);
    try {
      await apiSwitchModel(session.id, modelId);
      const updated = { ...session, agent_model: modelId };
      setCurrentSession(updated);
      
      setSessions(produce((list) => {
        const idx = list.findIndex((s) => s.id === session.id);
        if (idx !== -1) {
          list[idx] = updated;
        }
      }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to switch model';
      setError(msg);
      console.error('Failed to switch model:', err);
    }
  };

  const setSessionTitle = async (title: string) => {
    const session = currentSession();
    if (!session) return;

    setError(null);
    try {
      await apiSetSessionTitle(session.id, title);
      const updated = { ...session, title };
      setCurrentSession(updated);
      
      setSessions(produce((list) => {
        const idx = list.findIndex((s) => s.id === session.id);
        if (idx !== -1) {
          list[idx] = updated;
        }
      }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to set session title';
      setError(msg);
      console.error('Failed to set session title:', err);
    }
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
};

export function useSessionSafe(): SessionContextValue {
  const context = useContext(SessionContext);
  return context ?? fallbackSessionContext;
}
