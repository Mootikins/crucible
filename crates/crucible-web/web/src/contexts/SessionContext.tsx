import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
} from 'solid-js';
import type { Session, CreateSessionParams, ProviderInfo } from '@/lib/types';
import type { SessionContextValue } from '@/lib/types/context';
import { treeRootActions } from '@/stores/treeRootStore';
import {
  listModels as apiListModels,
  switchModel as apiSwitchModel,
  listProviders as apiListProviders,
} from '@/lib/api';
import type { SessionScope } from '@/lib/api';
import {
  dropCachedSession,
  fetchSessionOnce,
  patchCachedSession,
  useArchiveSession,
  useCancelSession,
  useCreateSession,
  useDeleteSession,
  useEndSession,
  usePauseSession,
  useResumeSession,
  useSessions,
  useSetSessionTitle,
  useUnarchiveSession,
} from '@/lib/query/sessions';
import { notificationActions } from '@/stores/notificationStore';
import { getBus } from '@/lib/bus';
import { setPendingFirstMessage } from '@/lib/draft-session';
import { tabHost } from '@/lib/tab-host';
import { statusBarStore } from '@/stores/statusBarStore';


interface SessionProviderProps {
  initialKiln?: string;
  initialWorkspace?: string;
  children: any;
}

const SessionContext = createContext<SessionContextValue>();

/**
 * Which session the shell is pointed at, and the actions that change one.
 *
 * The ROSTER is no longer held here: `useSessions()` owns it, so this context,
 * the inbox and the rail read one list from one fetch, and a session deleted
 * in the inbox leaves the rail at once. What stays is this client's own state
 * — the current selection, and which list variant the open panel asked for.
 *
 * Every mutation below is the shared hook plus this context's toast. The
 * optimistic splices the store used to do live in those hooks, so the inbox
 * gets them too rather than re-implementing them.
 */
export const SessionProvider: ParentComponent<SessionProviderProps> = (props) => {
  const [currentSession, setCurrentSession] = createSignal<Session | null>(null);
  const [availableModels, setAvailableModels] = createSignal<string[]>([]);
  const [providers, setProviders] = createSignal<ProviderInfo[]>([]);
  const [providersLoaded, setProvidersLoaded] = createSignal(false);
  const [selectedProvider, setSelectedProvider] = createSignal<ProviderInfo | null>(null);
  // The failure of one action, which is not the failure of the roster read.
  const [actionError, setActionError] = createSignal<string | null>(null);

  // Which list the open panel asked for. It is part of the query key, so both
  // variants are cached side by side and a bare refresh cannot replace the
  // archived view with the active one.
  const [includeArchived, setIncludeArchived] = createSignal(false);

  const sessionsQuery = useSessions(includeArchived);
  const sessions = () => sessionsQuery.data ?? [];

  const create = useCreateSession();
  const pause = usePauseSession();
  const resume = useResumeSession();
  const end = useEndSession();
  const remove = useDeleteSession();
  const archive = useArchiveSession();
  const unarchive = useUnarchiveSession();
  const cancel = useCancelSession();
  const rename = useSetSessionTitle();

  const [isCreating, setIsCreating] = createSignal(false);
  const isLoading = () => isCreating() || sessionsQuery.isFetching;
  const error = () => actionError() ?? sessionsQuery.error?.message ?? null;

  // A refused list read reaches the user as a toast, as it did when the
  // context fetched it. The query surfaces it once per failed fetch, because
  // `queryClientOptions` turns retries off.
  createEffect(() => {
    const failure = sessionsQuery.error;
    if (failure) {
      notificationActions.addNotification('error', failure.message);
      console.error('Failed to refresh sessions:', failure);
    }
  });

  // The file tree's per-session root pin outlives the session that owns it,
  // and the map is written on every root pick, so without this it only ever
  // grows. Only the archived-including list may prune: the default list omits
  // archived sessions, and pruning against it would forget their pins.
  createEffect(() => {
    const list = sessionsQuery.data;
    if (list && includeArchived()) treeRootActions.prune(list.map((s) => s.id));
  });

  /**
   * Asks the daemon for the list again, or for the other variant of it.
   *
   * A variant the caller names is a different key, and a key with no fresh
   * answer fetches on its own — so the switch IS the refresh. A bare call
   * keeps the variant on screen and re-reads it.
   */
  const refreshSessions = async (filters?: { includeArchived?: boolean }) => {
    const wanted = filters?.includeArchived;
    if (wanted !== undefined && wanted !== includeArchived()) {
      setIncludeArchived(wanted);
      return;
    }
    await sessionsQuery.refetch();
  };

  /** Applies one patch to every cached copy of a session, and to the selection. */
  const patchSessionById = (sessionId: string, patch: Partial<Session>): void => {
    patchCachedSession(sessionId, patch);
    const current = currentSession();
    if (current?.id === sessionId) setCurrentSession({ ...current, ...patch });
  };

  // Kiln/workspace mutations echo the updated scope; fold it into the cache
  // so chips and headers re-render without a refetch.
  const applySessionScope = (scope: SessionScope) => {
    patchSessionById(scope.session_id, {
      kilns: scope.kilns,
      workspace: scope.workspace,
    });
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
    setActionError(null);

    try {
      const result = await action();
      if (options.successMessage) {
        notificationActions.addNotification('success', options.successMessage);
      }
      return result;
    } catch (err) {
      const msg = err instanceof Error ? err.message : options.errorMessage;
      setActionError(msg);
      notificationActions.addNotification('error', msg);
      console.error(`${options.logPrefix}:`, err);
      if (options.rethrow) {
        throw err;
      }
      return undefined;
    }
  };

  const createSession = async (
    params: CreateSessionParams,
    opts?: { initialMessage?: string; model?: string },
  ): Promise<Session> => {
    setIsCreating(true);
    try {
      const session = await withSessionAction(async () => {
        const created = await create.mutateAsync(params);
        // Model choice from the draft surface: reuse the daemon's switch-model
        // resolution rather than parsing "provider_key/model" strings here.
        if (opts?.model) {
          try {
            await apiSwitchModel(created.id, opts.model);
            created.agent_model = opts.model;
            patchCachedSession(created.id, { agent_model: opts.model });
          } catch (err) {
            notificationActions.addNotification(
              'error',
              'Failed to set model — session uses the provider default'
            );
            console.error('Failed to set model on new session:', err);
          }
        }
        setCurrentSession(created);
        // Must be staged BEFORE open-session mounts the ChatProvider that
        // consumes it (lazy creation: draft surface → first message).
        if (opts?.initialMessage) {
          setPendingFirstMessage(created.id, opts.initialMessage);
        }
        window.dispatchEvent(new CustomEvent('crucible:open-session', {
          detail: { sessionId: created.id, title: created.title || 'New Session' },
        }));
        // Non-blocking: the model list is picker chrome — don't hold the
        // draft surface (and the first message) hostage to a models.list RPC.
        void refreshModels(created);
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
      setIsCreating(false);
    }
  };

  /**
   * Make `id` the current session without opening a tab for it.
   *
   * `selectSession` is the *user-initiated* path: it dispatches
   * `crucible:open-session`, which mounts a tab. Adoption is the reverse
   * direction — a pane that already exists telling the context to catch up —
   * so it must not re-dispatch, or a restored tab would try to open itself.
   */
  const adoptSession = async (id: string) => {
    if (currentSession()?.id === id) return;
    try {
      // A read, through the same key the pane's own bootstrap reads, so a
      // restored pane and this context ask the daemon once between them. The
      // daemon answers `session.get` for a stored session as for a live one,
      // so no history read is needed to bring it back first.
      const session = await fetchSessionOnce(id);
      // Focus can move again while the fetch is in flight; the last pane to be
      // focused wins, not the last response to land.
      if (statusBarStore.activeSessionId() !== id) return;
      setCurrentSession(session);
      await refreshModels(session);
    } catch {
      // The pane renders its own load failure; leaving the previous session in
      // place beats blanking the composer on a transient error.
    }
  };

  // A pane restored from the persisted layout (page reload) or brought into
  // focus by a tab switch announces itself through `activeSessionId` — it
  // never calls selectSession. Without this the pane renders a live session
  // while the composer stays disabled on "Select a session first…".
  createEffect(() => {
    const id = statusBarStore.activeSessionId();
    if (id) void adoptSession(id);
  });

  const selectSession = async (id: string) => {
    const existing = sessions().find((s) => s.id === id);
    if (existing) {
      if (!(await fetchSessionOnce(existing.id).then(() => true).catch(() => false))) {
        // The row can come from the last-known localStorage seed — the daemon
        // may have deleted the session since. The daemon reads a stored
        // session too, so a failed read means it is really gone: drop the dead
        // row instead of opening a chat tab that can never load.
        dropCachedSession(id);
        notificationActions.addNotification('error', 'Session no longer exists');
        return;
      }

      setCurrentSession(existing);
      // Transparently resume idle sessions on open. Paused sessions resume
      // warm; ended/evicted sessions revive from storage (the daemon route
      // falls back to session.resume_from_storage). Either way the opened
      // session is live so the composer is never a dead end.
      if (existing.state === 'paused' || existing.state === 'ended') {
        try {
          await resume.mutateAsync(id);
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

    setActionError(null);

    try {
      const session = await fetchSessionOnce(id);
      setCurrentSession(session);
      // Transparently resume idle sessions on open (paused warm, ended/evicted
      // from storage) so the opened session is always live.
      if (session.state === 'paused' || session.state === 'ended') {
        try {
          await resume.mutateAsync(id);
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
      setActionError(msg);
      console.error('Failed to select session:', err);
    }
  };

  /** The selection's own copy of a state the mutation already cached. */
  const updateCurrentSessionState = (state: Session['state']) => {
    const session = currentSession();
    if (!session) return;
    setCurrentSession({ ...session, state });
  };

  const pauseSession = async () => {
    const session = currentSession();
    if (!session) return;

    await withSessionAction(async () => {
      await pause.mutateAsync(session.id);
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
      await resume.mutateAsync(session.id);
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
      await end.mutateAsync(session.id);
      setCurrentSession(null);
    }, {
      errorMessage: 'Failed to end session',
      logPrefix: 'Failed to end session',
    });
  };

  /** Closes the chat tab of a session that is no longer listed. */
  const closeTabFor = (sessionId: string) => {
    const openTab = tabHost().find((t) => t.metadata?.sessionId === sessionId);
    if (openTab) tabHost().remove(openTab.id);
    if (currentSession()?.id === sessionId) setCurrentSession(null);
  };

  const deleteSession = async (sessionId: string) => {
    if (!confirm('Delete this session? This cannot be undone.')) return;

    await withSessionAction(async () => {
      await remove.mutateAsync(sessionId);
      closeTabFor(sessionId);
    }, {
      errorMessage: 'Failed to delete session',
      successMessage: 'Session deleted',
      logPrefix: 'Failed to delete session',
    });
  };

  const archiveSession = async (sessionId: string) => {
    await withSessionAction(async () => {
      await archive.mutateAsync(sessionId);
      closeTabFor(sessionId);
    }, {
      errorMessage: 'Failed to archive session',
      successMessage: 'Session archived',
      logPrefix: 'Failed to archive session',
    });
  };

  const unarchiveSession = async (sessionId: string) => {
    await withSessionAction(async () => {
      await unarchive.mutateAsync(sessionId);
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
      return await cancel.mutateAsync(session.id);
    } catch (err) {
      console.error('Failed to cancel operation:', err);
      return false;
    }
  };

  // Only the most recent `refreshModels` call may write the list.
  //
  // Without this, two overlapping calls can resolve in reverse order and the
  // older one wins — or the no-session `[]` path lands after a populated list.
  // Either way the picker goes stale or empty after having been correct, which
  // is a race the user sees as options vanishing mid-click. The model list is
  // still a bare call here; Task C2.4 gives it a key, and the key retires this
  // guard as it retired the list's.
  let modelsGeneration = 0;

  const refreshModels = async (sessionOverride?: Session) => {
    const generation = ++modelsGeneration;
    const isCurrent = () => generation === modelsGeneration;
    // Applied only while this call is still the newest.
    const publish = (models: string[]) => {
      if (isCurrent()) {
        setAvailableModels(models);
      }
    };

    const session = sessionOverride ?? currentSession();
    if (!session?.id) {
      publish([]);
      return;
    }

    // Provider models are the fallback when the session has no agent
    // configured, or when the lookup fails outright.
    const providerFallback = () =>
      (selectedProvider() ?? providers()[0])?.models ?? [];

    try {
      const models = await apiListModels(session.id);
      publish(models.length > 0 ? models : providerFallback());
    } catch (err) {
      const msg = 'Failed to load models';
      notificationActions.addNotification('error', msg);
      console.error(msg, err);
      publish(providerFallback());
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
      await rename.mutateAsync({ id: session.id, title });
      setCurrentSession({ ...session, title });
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
    } finally {
      setProvidersLoaded(true);
    }
  };

  const selectProvider = (providerType: string) => {
    const provider = providers().find((p) => p.provider_type === providerType);
    if (provider) {
      setSelectedProvider(provider);
    }
  };

  // The roster is NOT fetched here any more. `useSessions()` reads it on
  // mount, and the list never depended on the kiln: the tree groups and
  // filters the global list on the client.
  createEffect(() => {
    if (!props.initialKiln) return; // Guard: skip until config has loaded
    refreshProviders();
  });

  // Daemon auto-titles sessions on their first completed turn; the chat
  // stream's route announces the new title so the selection stays current.
  // The route also invalidates both list keys, so the rows correct themselves;
  // the patch here is what paints before that answer lands.
  const onTitleChangedEvent = ({ sessionId, title }: { sessionId: string; title: string }) => {
    patchSessionById(sessionId, { title });
  };
  // `on` removes the handler with this owner, so the provider needs no
  // `onCleanup` of its own.
  getBus().on('sessionTitleChanged', onTitleChangedEvent);

  const value: SessionContextValue = {
    currentSession,
    sessions,
    isLoading,
    error,
    availableModels,
    providers,
    providersLoaded,
    selectedProvider,
    createSession,
    selectSession,
    applySessionScope,
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
  providersLoaded: () => false,
  selectedProvider: () => null,
  createSession: () => Promise.reject(new Error('No session context')),
  applySessionScope: () => {},
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
