import { Component, For, Show, createMemo, createSignal } from 'solid-js';
import { SECTION_LABEL_CLASS } from '@/components/ui/SectionLabel';
import { useSessionSafe } from '@/contexts/SessionContext';
import { attentionStore, attentionActions, type SessionAttention } from '@/stores/attentionStore';
import { InteractionHandler } from '@/components/interactions';
import { respondToInteraction } from '@/lib/api';
import { useDeleteSession, useSessions, useUnarchiveSession } from '@/lib/query/sessions';
import { sortByRecency, sessionDisplayTitle } from '@/lib/session-display';
import { relativeTime } from '@/lib/format-time';
import { sessionStatus, type SessionStatus } from '@/lib/session-status';
import { SessionStatusDot } from '@/components/shell/SessionStatusDot';
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

const LIVE_LABEL: Record<SessionStatus, string | null> = {
  waiting: 'WAITING',
  working: 'STREAMING',
  // Nothing live to say — the lifecycle axis below names it instead.
  idle: null,
};

/**
 * The word beside a row.
 *
 * TWO axes, not one. `sessionStatus` answers what the session is doing right
 * now; `session.state` answers where it is in its lifecycle. This file used to
 * own a second function of the same name over the same live states, with its
 * own colours — streaming pulsed green here and read as filled brass in the
 * switcher — so one session wore two vocabularies depending on the panel. The
 * live axis now comes from `lib/session-status.ts` and renders through
 * `SessionStatusDot`, which is the same dot the rail and the switcher draw.
 */
function statusLabel(session: Session): { label: string; color: string } {
  const live = LIVE_LABEL[sessionStatus(session)];
  if (live) {
    return {
      label: live,
      color: sessionStatus(session) === 'waiting' ? 'text-attention' : 'text-ok',
    };
  }
  return STATE_DISPLAY[session.state] ?? STATE_DISPLAY.active;
}

const InboxPanel: Component = () => {
  const sessionCtx = useSessionSafe();
  const [resolved, setResolved] = createSignal<string | null>(null);
  const [archivedOpen, setArchivedOpen] = createSignal(false);
  // ONE list, and the disclosure above IS its flag: closed, this panel reads
  // the very list the rail reads and pays for no fetch of its own; open, it
  // asks the daemon the wider question once. It used to keep a second copy
  // behind its own resource and refetch only that copy, so a session it
  // deleted stayed in the rail, and a session the rail created was missing
  // here until this panel remounted. The mutations are the shared ones, so
  // every reader of the list sees a delete or a restore at once.
  const sessions = useSessions(archivedOpen);
  const remove = useDeleteSession();
  const restore = useUnarchiveSession();
  const [clearArmed, setClearArmed] = createSignal(false);
  const [clearProgress, setClearProgress] = createSignal<string | null>(null);
  const [pendingDelete, setPendingDelete] = createSignal<string | null>(null);

  const waiting = attentionStore.waiting;

  const rows = () => sessions.data ?? [];
  const recentSessions = createMemo(() => sortByRecency(rows().filter((s) => !s.archived)));
  const archived = createMemo(() => sortByRecency(rows().filter((s) => s.archived)));

  const titleFor = (entry: SessionAttention) => {
    if (entry.title) return entry.title;
    const session = rows().find((s) => s.id === entry.sessionId);
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

  // Each mutation writes the shared list, so nothing here refetches: the row
  // leaves this panel and the rail together.
  const restoreSession = async (sessionId: string) => {
    await restore.mutateAsync(sessionId).catch(() => {});
  };

  const deleteArchived = async (sessionId: string) => {
    if (pendingDelete() !== sessionId) {
      setPendingDelete(sessionId);
      return;
    }
    setPendingDelete(null);
    await remove.mutateAsync(sessionId).catch(() => {});
  };

  const clearArchived = async () => {
    if (!clearArmed()) {
      setClearArmed(true);
      return;
    }
    setClearArmed(false);
    const targets = archived();
    let done = 0;
    for (const session of targets) {
      setClearProgress(`Deleting ${done + 1}/${targets.length}…`);
      // Keep going — a single failed delete shouldn't strand the rest.
      await remove.mutateAsync(session.id).catch(() => {});
      done += 1;
    }
    setClearProgress(null);
  };

  const SessionRow = (rowProps: { session: Session; archivedRow: boolean }) => {
    const session = rowProps.session;
    const status = () => statusLabel(session);
    return (
      <div class="w-full flex items-center gap-2.5 px-3 py-2.5 rounded-lg border border-hairline mb-1.5 hover:bg-surface-elevated hover:border-primary/40 transition-colors group">
        <SessionStatusDot status={sessionStatus(session)} labelled />
        <button
          type="button"
          class="flex-1 min-w-0 text-left cursor-pointer"
          onClick={() => openSession(session.id)}
        >
          <span class="block text-reading font-semibold truncate">
            {sessionDisplayTitle(session)}
          </span>
          <span class="block text-floor text-muted-dark truncate">
            {relativeTime(session.last_activity ?? session.started_at)}
            {session.agent_model ? ` · ${session.agent_model}` : ''}
            {session.event_count ? ` · ${session.event_count} events` : ''}
          </span>
        </button>
        <Show
          when={rowProps.archivedRow}
          fallback={
            <span class={`font-mono text-floor font-medium flex-none ${status().color}`}>
              {status().label}
            </span>
          }
        >
          <button
            type="button"
            class="font-mono text-floor text-muted-dark hover:text-ok cursor-pointer flex-none opacity-0 group-hover:opacity-100 transition-opacity"
            title="Restore to recent sessions"
            onClick={() => void restoreSession(session.id)}
          >
            RESTORE
          </button>
          <button
            type="button"
            class={`font-mono text-floor cursor-pointer flex-none transition-opacity ${
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
        <div class="font-mono text-floor text-muted-dark mb-4">
          {attentionStore.attentionCount()} pending · {recentSessions().length} recent sessions
        </div>

        <For each={waiting()}>
          {(entry) => (
            <div class="bg-attention/5 border border-attention/40 rounded-lg px-3.5 py-3 mb-2.5">
              <div class="flex items-center gap-2 mb-2">
                <SessionStatusDot status="waiting" labelled />
                <span class="text-reading font-semibold">{titleFor(entry)}</span>
                <span class="flex-1" />
                <button
                  type="button"
                  class="text-muted-dark text-floor hover:text-muted cursor-pointer"
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
          <div class="flex items-center gap-2.5 bg-ok/5 border border-ok/30 rounded-lg px-3.5 py-2.5 mb-2.5 text-ok text-reading">
            ✓ all clear — nothing waiting on you
          </div>
        </Show>

        <Show when={resolved()}>
          <div class="border border-ok/30 bg-ok/5 rounded-lg px-3 py-2 mb-2.5 text-floor text-ok">
            {resolved()}
          </div>
        </Show>

        <div class={`${SECTION_LABEL_CLASS} pt-2.5 pb-2`}>
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
            <div class="text-muted-dark font-mono text-floor px-1 pb-2">
              showing {RECENT_CAP} of {recentSessions().length}
            </div>
          </Show>
        </Show>

        <button
          type="button"
          data-testid="archived-toggle"
          class={`w-full flex items-center gap-2 ${SECTION_LABEL_CLASS} pt-3 pb-2 cursor-pointer hover:text-muted transition-colors`}
          onClick={() => setArchivedOpen(!archivedOpen())}
        >
          <span>{archivedOpen() ? '▾' : '▸'}</span>
          <span>ARCHIVED</span>
          <Show when={archivedOpen()}>
            <span>({archived().length})</span>
          </Show>
          <span class="flex-1" />
          <span class="font-normal normal-case tracking-normal text-floor">
            idle sessions are archived automatically after 3 days
          </span>
        </button>

        <Show when={archivedOpen()}>
          <Show when={!sessions.isPending} fallback={<div class="text-muted-dark text-xs px-1 py-2">Loading…</div>}>
            <Show
              when={archived().length > 0}
              fallback={<div class="text-muted-dark text-xs px-1 py-2">Nothing archived.</div>}
            >
              <div class="flex items-center gap-2 pb-2">
                <button
                  type="button"
                  data-testid="clear-archived"
                  class={`font-mono text-floor border rounded-md px-2.5 py-1 cursor-pointer transition-colors ${
                    clearArmed()
                      ? 'border-error text-error'
                      : 'border-hairline text-muted-dark hover:text-error hover:border-error/50'
                  }`}
                  onBlur={() => setClearArmed(false)}
                  onClick={() => void clearArchived()}
                >
                  {clearArmed()
                    ? `REALLY DELETE ${archived().length} SESSIONS?`
                    : 'CLEAR HISTORY…'}
                </button>
                <Show when={clearProgress()}>
                  <span class="font-mono text-floor text-muted-dark">{clearProgress()}</span>
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
