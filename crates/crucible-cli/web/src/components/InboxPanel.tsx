import { Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { attentionStore, attentionActions, type SessionAttention } from '@/stores/attentionStore';
import { InteractionHandler } from '@/components/interactions';
import { respondToInteraction, listSessions, deleteSession, unarchiveSession } from '@/lib/api';
import { sortByRecency, sessionDisplayTitle } from '@/lib/session-display';
import { relativeTime } from '@/components/HomePanel';
import type { InteractionResponse, Session, SessionState } from '@/lib/types';

// ── Inbox — everything waiting on you, one place ─────────────────────────
// Crucible Shell design turn 5 / Feature Spec §3.2 "Agent Inbox". Pending
// interactions (permissions, asks, popups) from every session are
// answerable here without switching tabs; below them, recent sessions by
// last activity, with idle sessions auto-archived by the daemon into a
// collapsed ARCHIVED section (restore / delete / clear history).

const RECENT_CAP = 30;

const STATE_DISPLAY: Record<SessionState, { label: string; color: string }> = {
  active: { label: 'ACTIVE', color: 'text-ok' },
  paused: { label: 'PAUSED', color: 'text-muted-dark' },
  compacting: { label: 'COMPACTING', color: 'text-attention' },
  ended: { label: 'ENDED', color: 'text-muted-dark' },
};

function sessionStatus(session: Session, attention: SessionAttention | undefined) {
  if (attention?.pendingInteraction) {
    return { label: 'WAITING', color: 'text-attention', dot: 'bg-attention animate-pulse' };
  }
  if (attention?.isStreaming) {
    return { label: 'STREAMING', color: 'text-ok', dot: 'bg-ok animate-pulse' };
  }
  const display = STATE_DISPLAY[session.state] ?? STATE_DISPLAY.active;
  return {
    ...display,
    dot: session.state === 'active' ? 'bg-ok' : 'bg-muted-dark',
  };
}

export const InboxPanel: Component = () => {
  const sessionCtx = useSessionSafe();
  const [resolved, setResolved] = createSignal<string | null>(null);
  const [archivedOpen, setArchivedOpen] = createSignal(false);
  const [clearArmed, setClearArmed] = createSignal(false);
  const [clearProgress, setClearProgress] = createSignal<string | null>(null);
  const [pendingDelete, setPendingDelete] = createSignal<string | null>(null);

  const waiting = attentionStore.waiting;

  const recentSessions = createMemo(() =>
    sortByRecency(sessionCtx.sessions().filter((s) => !s.archived))
  );

  // Archived sessions load lazily on first expand; the daemon hides them
  // from the default listing.
  const [archived, { refetch: refetchArchived }] = createResource(
    archivedOpen,
    async (open) => {
      if (!open) return [] as Session[];
      const all = await listSessions({ includeArchived: true }).catch(() => [] as Session[]);
      return sortByRecency(all.filter((s) => s.archived));
    }
  );

  const titleFor = (entry: SessionAttention) => {
    if (entry.title) return entry.title;
    const session = sessionCtx.sessions().find((s) => s.id === entry.sessionId);
    return session ? sessionDisplayTitle(session) : `Session ${entry.sessionId.slice(-8)}`;
  };

  const respond = async (entry: SessionAttention, response: InteractionResponse) => {
    const request = entry.pendingInteraction;
    if (!request) return;
    try {
      await respondToInteraction(entry.sessionId, request.id, response);
      // Tell the owning ChatProvider (if a tab is open) its interaction is
      // handled so the in-chat prompt disappears too.
      window.dispatchEvent(
        new CustomEvent('crucible:interaction-resolved', {
          detail: { sessionId: entry.sessionId, requestId: request.id },
        })
      );
      attentionActions.resolveInteraction(entry.sessionId, request.id);
      // Re-sync the daemon aggregate — the responded entry is gone there.
      void attentionActions.refresh();
      setResolved(`✓ Resolved — ${titleFor(entry)}`);
    } catch (err) {
      setResolved(
        `✕ Failed to respond: ${err instanceof Error ? err.message : 'unknown error'}`
      );
    }
  };

  const openSession = (sessionId: string) => {
    void sessionCtx.selectSession(sessionId).catch(() => {});
  };

  const restoreSession = async (sessionId: string) => {
    try {
      await unarchiveSession(sessionId);
    } finally {
      void refetchArchived();
      void sessionCtx.refreshSessions();
    }
  };

  const deleteArchived = async (sessionId: string) => {
    if (pendingDelete() !== sessionId) {
      setPendingDelete(sessionId);
      return;
    }
    setPendingDelete(null);
    try {
      await deleteSession(sessionId);
    } finally {
      void refetchArchived();
    }
  };

  const clearArchived = async () => {
    if (!clearArmed()) {
      setClearArmed(true);
      return;
    }
    setClearArmed(false);
    const targets = archived() ?? [];
    let done = 0;
    for (const session of targets) {
      setClearProgress(`Deleting ${done + 1}/${targets.length}…`);
      try {
        await deleteSession(session.id);
      } catch {
        // keep going — a single failed delete shouldn't strand the rest
      }
      done += 1;
    }
    setClearProgress(null);
    void refetchArchived();
    void sessionCtx.refreshSessions();
  };

  const SessionRow = (rowProps: { session: Session; archivedRow: boolean }) => {
    const session = rowProps.session;
    const status = () => sessionStatus(session, attentionStore.get(session.id));
    return (
      <div class="w-full flex items-center gap-2.5 px-3 py-2.5 rounded-lg border border-white/5 mb-1.5 hover:bg-surface-elevated hover:border-primary/40 transition-colors group">
        <span class={`w-2 h-2 rounded-full flex-none ${status().dot}`} />
        <button
          type="button"
          class="flex-1 min-w-0 text-left cursor-pointer"
          onClick={() => openSession(session.id)}
        >
          <span class="block text-[12.5px] font-semibold truncate">
            {sessionDisplayTitle(session)}
          </span>
          <span class="block text-[11px] text-muted-dark truncate">
            {relativeTime(session.last_activity ?? session.started_at)}
            {session.agent_model ? ` · ${session.agent_model}` : ''}
            {session.event_count ? ` · ${session.event_count} events` : ''}
          </span>
        </button>
        <Show
          when={rowProps.archivedRow}
          fallback={
            <span class={`font-mono text-[10px] font-medium flex-none ${status().color}`}>
              {status().label}
            </span>
          }
        >
          <button
            type="button"
            class="font-mono text-[10px] text-muted-dark hover:text-ok cursor-pointer flex-none opacity-0 group-hover:opacity-100 transition-opacity"
            title="Restore to recent sessions"
            onClick={() => void restoreSession(session.id)}
          >
            RESTORE
          </button>
          <button
            type="button"
            class={`font-mono text-[10px] cursor-pointer flex-none transition-opacity ${
              pendingDelete() === session.id
                ? 'text-error opacity-100'
                : 'text-muted-dark hover:text-error opacity-0 group-hover:opacity-100'
            }`}
            title="Delete session permanently"
            onBlur={() => setPendingDelete(null)}
            onClick={() => void deleteArchived(session.id)}
          >
            {pendingDelete() === session.id ? 'SURE?' : 'DELETE'}
          </button>
        </Show>
      </div>
    );
  };

  return (
    <div class="h-full overflow-y-auto bg-shell-bg text-shell-ink">
      <div class="max-w-[660px] mx-auto px-6 py-5">
        <div class="text-base font-bold mb-1">Inbox</div>
        <div class="font-mono text-[10.5px] text-muted-dark mb-4">
          {attentionStore.attentionCount()} pending · {recentSessions().length} recent sessions
        </div>

        <For each={waiting()}>
          {(entry) => (
            <div class="bg-attention/5 border border-attention/40 rounded-lg px-3.5 py-3 mb-2.5">
              <div class="flex items-center gap-2 mb-2">
                <span class="w-[7px] h-[7px] rounded-full bg-attention animate-pulse" />
                <span class="text-[12.5px] font-semibold">{titleFor(entry)}</span>
                <span class="flex-1" />
                <button
                  type="button"
                  class="text-muted-dark text-[11px] hover:text-muted cursor-pointer"
                  onClick={() => openSession(entry.sessionId)}
                >
                  open session →
                </button>
              </div>
              <InteractionHandler
                request={entry.pendingInteraction!}
                onRespond={(response) => void respond(entry, response)}
              />
            </div>
          )}
        </For>

        <Show when={waiting().length === 0}>
          <div class="flex items-center gap-2.5 bg-ok/5 border border-ok/30 rounded-lg px-3.5 py-2.5 mb-2.5 text-ok text-[12.5px]">
            ✓ all clear — nothing waiting on you
          </div>
        </Show>

        <Show when={resolved()}>
          <div class="border border-ok/30 bg-ok/5 rounded-lg px-3 py-2 mb-2.5 text-[11.5px] text-ok">
            {resolved()}
          </div>
        </Show>

        <div class="font-mono font-semibold text-[10px] tracking-[0.08em] text-muted-dark pt-2.5 pb-2">
          RECENT SESSIONS
        </div>
        <Show
          when={recentSessions().length > 0}
          fallback={
            <div class="text-muted-dark text-xs px-1 py-2">
              No recent sessions — start one from Home or the header.
            </div>
          }
        >
          <For each={recentSessions().slice(0, RECENT_CAP)}>
            {(session) => <SessionRow session={session} archivedRow={false} />}
          </For>
          <Show when={recentSessions().length > RECENT_CAP}>
            <div class="text-muted-dark font-mono text-[10px] px-1 pb-2">
              showing {RECENT_CAP} of {recentSessions().length}
            </div>
          </Show>
        </Show>

        <button
          type="button"
          data-testid="archived-toggle"
          class="w-full flex items-center gap-2 font-mono font-semibold text-[10px] tracking-[0.08em] text-muted-dark pt-3 pb-2 cursor-pointer hover:text-muted transition-colors"
          onClick={() => setArchivedOpen(!archivedOpen())}
        >
          <span>{archivedOpen() ? '▾' : '▸'}</span>
          <span>ARCHIVED</span>
          <Show when={archivedOpen() && archived()}>
            <span>({archived()!.length})</span>
          </Show>
          <span class="flex-1" />
          <span class="font-normal normal-case tracking-normal text-[10px]">
            idle sessions are archived automatically after 3 days
          </span>
        </button>

        <Show when={archivedOpen()}>
          <Show when={!archived.loading} fallback={<div class="text-muted-dark text-xs px-1 py-2">Loading…</div>}>
            <Show
              when={(archived() ?? []).length > 0}
              fallback={<div class="text-muted-dark text-xs px-1 py-2">Nothing archived.</div>}
            >
              <div class="flex items-center gap-2 pb-2">
                <button
                  type="button"
                  data-testid="clear-archived"
                  class={`font-mono text-[10px] border rounded-md px-2.5 py-1 cursor-pointer transition-colors ${
                    clearArmed()
                      ? 'border-error text-error'
                      : 'border-white/10 text-muted-dark hover:text-error hover:border-error/50'
                  }`}
                  onBlur={() => setClearArmed(false)}
                  onClick={() => void clearArchived()}
                >
                  {clearArmed()
                    ? `REALLY DELETE ${(archived() ?? []).length} SESSIONS?`
                    : 'CLEAR HISTORY…'}
                </button>
                <Show when={clearProgress()}>
                  <span class="font-mono text-[10px] text-muted-dark">{clearProgress()}</span>
                </Show>
              </div>
              <For each={archived()}>
                {(session) => <SessionRow session={session} archivedRow={true} />}
              </For>
            </Show>
          </Show>
        </Show>
      </div>
    </div>
  );
};

export default InboxPanel;
