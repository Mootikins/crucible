import { Component, For, Show, createEffect, createMemo } from 'solid-js';
import { Message } from './Message';
import { AssistantTurn, type TurnPartSpec } from './AssistantTurn';
import { InteractionHandler } from './interactions';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';

/**
 * Transcript row. A TURN groups everything the agent did for one prompt —
 * interleaved text segments and tool-call runs — into a single block with
 * one meta row (timestamp/usage), instead of scattering chrome across
 * segments. Rows carry message IDS only, never message objects: content
 * changes resolve inside the part components via store lookups, so a
 * streamed token causes zero row/wrapper churn — rows change only when the
 * transcript's STRUCTURE (id/role sequence) changes.
 */
type TranscriptRow =
  | { kind: 'message'; id: string }
  | { kind: 'turn'; key: string; parts: TurnPartSpec[] };

export const MessageList: Component = () => {
  const { messages, pendingInteraction, respondToInteraction } = useChatSafe();
  const { currentSession } = useSessionSafe();
  let containerRef: HTMLDivElement | undefined;
  let bottomRef: HTMLDivElement | undefined;

  // Auto-scroll only while the user is pinned at (near) the bottom — a reader
  // who scrolled up to their scrollback must not be yanked down on every
  // streamed token. Sending a message re-pins: your own prompt always comes
  // into view.
  let pinned = true;

  const handleScroll = () => {
    if (!containerRef) return;
    const distance =
      containerRef.scrollHeight - containerRef.scrollTop - containerRef.clientHeight;
    pinned = distance < 40;
  };

  const scrollToBottom = () => {
    if (pinned) bottomRef?.scrollIntoView({ behavior: 'instant', block: 'end' });
  };

  createEffect(() => {
    const msgs = messages();
    if (msgs[msgs.length - 1]?.role === 'user') pinned = true;
    pendingInteraction();
    queueMicrotask(scrollToBottom);
  });

  const session = () => currentSession();

  // <For> keys rows by reference. Rows are structural (ids only), so their
  // content never changes — but this memo re-runs on ANY store change, and a
  // rebuilt row would be a fresh reference that remounts its whole subtree.
  // Cache rows by a structural signature and reuse the identical wrapper
  // while the signature is unchanged; only genuine structure changes (new
  // message, id rename, role change) produce a new wrapper.
  const rowCache = new Map<string, TranscriptRow>();

  const rows = createMemo<TranscriptRow[]>(() => {
    // Build structural specs first.
    const specs: TranscriptRow[] = [];
    let turn: { parts: TurnPartSpec[] } | null = null;

    const closeTurn = () => {
      if (!turn) return;
      const firstPart = turn.parts[0];
      const key = firstPart.kind === 'text' ? firstPart.id : firstPart.ids[0];
      specs.push({ kind: 'turn', key: `turn-${key}`, parts: turn.parts });
      turn = null;
    };

    for (const message of messages()) {
      if (message.role === 'assistant') {
        turn ??= { parts: [] };
        turn.parts.push({ kind: 'text', id: message.id });
      } else if (message.role === 'tool') {
        turn ??= { parts: [] };
        const last = turn.parts[turn.parts.length - 1];
        if (last?.kind === 'tools') {
          last.ids.push(message.id);
        } else {
          turn.parts.push({ kind: 'tools', key: `tools-${message.id}`, ids: [message.id] });
        }
      } else {
        closeTurn();
        specs.push({ kind: 'message', id: message.id });
      }
    }
    closeTurn();

    // Reuse cached wrappers with identical structure.
    const signature = (row: TranscriptRow): string =>
      row.kind === 'message'
        ? `m:${row.id}`
        : `t:${row.key}:${row.parts
            .map((p) => (p.kind === 'text' ? p.id : `[${p.ids.join(',')}]`))
            .join('|')}`;

    const seen = new Set<string>();
    const out = specs.map((spec) => {
      const sig = signature(spec);
      seen.add(sig);
      const cached = rowCache.get(sig);
      if (cached) return cached;
      rowCache.set(sig, spec);
      return spec;
    });

    // Evict stale signatures (clear, session switch, structure changes) so
    // the cache doesn't grow unbounded across a long-lived provider.
    for (const sig of rowCache.keys()) {
      if (!seen.has(sig)) rowCache.delete(sig);
    }
    return out;
  });

  return (
    <div
      ref={containerRef}
      onScroll={handleScroll}
      class="flex-1 overflow-y-auto px-4 py-4"
      data-testid="message-list"
    >
      <For each={rows()}>
        {(row) => {
          if (row.kind === 'turn') {
            const isLastTurn = createMemo(() => {
              const all = rows();
              for (let i = all.length - 1; i >= 0; i--) {
                if (all[i].kind === 'turn') return all[i] === row;
              }
              return false;
            });
            return <AssistantTurn parts={row.parts} isLast={isLastTurn()} />;
          }
          const message = createMemo(() => messages().find((m) => m.id === row.id));
          return (
            <Show when={message()}>
              <Message message={message()!} />
            </Show>
          );
        }}
      </For>

      {/* Permission/ask prompts appear inline at the point in the
          conversation where the agent is blocked, like other agent UIs. */}
      <Show when={pendingInteraction()}>
        {(request) => (
          <InteractionHandler request={request()} onRespond={respondToInteraction} />
        )}
      </Show>
      <div ref={bottomRef} class="h-px" />

      <Show when={messages().length === 0}>
        <div class="h-full flex flex-col items-center justify-center gap-4">
          <Show
            when={session()}
            fallback={
              <>
                <div class="w-16 h-16 rounded-full bg-surface-elevated flex items-center justify-center">
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                    class="w-8 h-8 text-muted-dark"
                  >
                    <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
                  </svg>
                </div>
                <p class="text-muted-dark text-center">
                  Select or create a session to start chatting
                </p>
              </>
            }
          >
            <>
              <div class="w-16 h-16 rounded-full bg-primary/15 flex items-center justify-center">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  class="w-8 h-8 text-primary"
                >
                  <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
                </svg>
              </div>
              <p class="text-muted text-center">
                Start a conversation by typing a message or using voice input
              </p>
              <Show when={session()?.agent_model}>
                <p class="text-muted-dark text-sm">
                  Model: {session()?.agent_model}
                </p>
              </Show>
            </>
          </Show>
        </div>
      </Show>
    </div>
  );
};
