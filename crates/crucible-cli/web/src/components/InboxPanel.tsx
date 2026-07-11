import { Component, For, Show, createSignal } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { attentionStore, attentionActions, type SessionAttention } from '@/stores/attentionStore';
import { InteractionHandler } from '@/components/interactions';
import { respondToInteraction } from '@/lib/api';
import type { InteractionResponse, Session, SessionState } from '@/lib/types';

// ── Inbox — everything waiting on you, one place ─────────────────────────
// Crucible Shell design turn 5 / Feature Spec §3.2 "Agent Inbox". Pending
// interactions (permissions, asks, popups) from every session with an open
// tab are answerable here without switching tabs; below them, every
// session with live status.

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

  const waiting = attentionStore.waiting;

  const titleFor = (entry: SessionAttention) =>
    entry.title ??
    sessionCtx.sessions().find((s) => s.id === entry.sessionId)?.title ??
    `Session ${entry.sessionId.slice(0, 8)}`;

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
      attentionActions.report(entry.sessionId, { pendingInteraction: null });
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

  return (
    <div class="h-full overflow-y-auto bg-shell-bg text-shell-ink">
      <div class="max-w-[660px] mx-auto px-6 py-5">
        <div class="text-base font-bold mb-1">Inbox</div>
        <div class="font-mono text-[10.5px] text-muted-dark mb-4">
          {attentionStore.attentionCount()} pending · {sessionCtx.sessions().length} sessions
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
          ALL SESSIONS
        </div>
        <Show
          when={sessionCtx.sessions().length > 0}
          fallback={
            <div class="text-muted-dark text-xs px-1 py-2">
              No sessions yet — start one from Home or the header.
            </div>
          }
        >
          <For each={sessionCtx.sessions()}>
            {(session) => {
              const status = () => sessionStatus(session, attentionStore.get(session.id));
              return (
                <button
                  type="button"
                  class="w-full flex items-center gap-2.5 px-3 py-2.5 rounded-lg cursor-pointer border border-white/5 mb-1.5 text-left hover:bg-surface-elevated hover:border-primary/40 transition-colors"
                  onClick={() => openSession(session.id)}
                >
                  <span class={`w-2 h-2 rounded-full flex-none ${status().dot}`} />
                  <span class="flex-1 min-w-0">
                    <span class="block text-[12.5px] font-semibold truncate">
                      {session.title || `Session ${session.id.slice(0, 8)}`}
                    </span>
                    <span class="block text-[11px] text-muted-dark truncate">
                      {session.agent_model || session.session_type}
                      {session.event_count ? ` · ${session.event_count} events` : ''}
                    </span>
                  </span>
                  <span class={`font-mono text-[10px] font-medium flex-none ${status().color}`}>
                    {status().label}
                  </span>
                </button>
              );
            }}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default InboxPanel;
