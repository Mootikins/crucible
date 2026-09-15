import { Component, For, Show, createEffect, createMemo, createSignal } from 'solid-js';
import { Message } from './Message';
import { AssistantTurn, type TurnPartSpec } from './AssistantTurn';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { InteractionRequest } from '@/lib/types';

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

/**
 * The empty state's chat-bubble mark. One definition, two callers: the
 * no-session and has-session branches differ only in tint, and the ~400
 * character Heroicons path was pasted into both.
 */
const ChatBubbleMark: Component<{ ring: string; glyph: string }> = (props) => (
  <div class={`w-16 h-16 rounded-full flex items-center justify-center ${props.ring}`}>
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="currentColor"
      class={`w-8 h-8 ${props.glyph}`}
    >
      <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
    </svg>
  </div>
);

/**
 * What the transcript says where the agent stopped to ask.
 *
 * One line, past tense, naming the thing asked for — the record of an event,
 * which is what a transcript holds. The card that answers it is docked on the
 * composer (see `ChatInput`), because a control the session is parked on must
 * not be able to scroll out of sight.
 */
function interactionRecord(request: InteractionRequest): string {
  switch (request.kind) {
    case 'permission': {
      const subject = request.tokens.join(' ') || request.tool_name || 'a tool';
      switch (request.action_type) {
        case 'write':
          return `Asked to write ${subject}`;
        case 'read':
          return `Asked to read ${subject}`;
        case 'bash':
          return `Asked to run ${subject}`;
        case 'tool':
          return `Asked to use ${request.tool_name || subject}`;
      }
      break;
    }
    case 'ask':
      return `Asked: ${request.question}`;
    case 'ask_batch':
      return `Asked ${request.questions.length} questions`;
    case 'edit':
      return 'Asked you to edit a document';
    case 'show':
      return `Showed ${request.title || 'a document'}`;
    case 'popup':
      return `Asked: ${request.title}`;
    case 'panel':
      return `Asked: ${request.header}`;
  }
  return 'Asked for your answer';
}

export const MessageList: Component = () => {
  const { messages, pendingInteraction } = useChatSafe();
  const { currentSession } = useSessionSafe();
  let containerRef: HTMLDivElement | undefined;
  let bottomRef: HTMLDivElement | undefined;

  // Auto-scroll only while the user is pinned at (near) the bottom — a reader
  // who scrolled up to their scrollback must not be yanked down on every
  // streamed token. Sending a message re-pins: your own prompt always comes
  // into view.
  let pinned = true;

  /**
   * Whether the transcript continues below the fold.
   *
   * This drives the bottom fade, which is the ONLY thing separating the
   * transcript from the composer now that the rule between them is gone. It
   * is a signal rather than the plain `pinned` flag because it paints: a soft
   * bottom edge means "there is more down there", and it must disappear the
   * moment you reach the end, or it reads as a permanent decoration and stops
   * carrying any information at all.
   */
  const [hasMoreBelow, setHasMoreBelow] = createSignal(false);

  const measure = () => {
    if (!containerRef) return;
    const distance =
      containerRef.scrollHeight - containerRef.scrollTop - containerRef.clientHeight;
    pinned = distance < 40;
    // A wider threshold than `pinned` uses: the fade is 3rem tall, so it has
    // to be gone before the last line slides under it, not exactly at zero.
    setHasMoreBelow(distance > 8);
  };

  const handleScroll = () => measure();

  const scrollToBottom = () => {
    if (pinned) bottomRef?.scrollIntoView({ behavior: 'instant', block: 'end' });
  };

  createEffect(() => {
    const msgs = messages();
    if (msgs[msgs.length - 1]?.role === 'user') pinned = true;
    pendingInteraction();
    queueMicrotask(() => {
      scrollToBottom();
      // Streaming grows the transcript without ever firing a scroll event, so
      // the fade would otherwise stay stale for a whole turn.
      measure();
    });
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
      // The SCROLLER is full width so its scrollbar stays on the pane edge;
      // the content inside it centres on `--chat-measure`. Centring the
      // scroller instead pulls the bar inward and reads as a second panel.
      //
      // `.transcript-fade` is conditional on purpose — see `hasMoreBelow`.
      classList={{
        'flex-1 overflow-y-auto px-4 pt-4 pb-2': true,
        'transcript-fade': hasMoreBelow(),
      }}
      data-testid="message-list"
    >
      <div class="mx-auto w-full max-w-[var(--chat-measure)]">
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

      {/* The record, at the point in the conversation where the agent stopped.
          The card that answers it is docked on the composer. */}
      <Show when={pendingInteraction()}>
        {(request) => (
          <div
            class="flex items-baseline gap-1.5 py-1 text-sm text-muted"
            data-testid="interaction-record"
          >
            <span class="min-w-0 truncate">{interactionRecord(request())}</span>
            <span class="shrink-0 text-attention">· waiting</span>
          </div>
        )}
      </Show>
      <div ref={bottomRef} class="h-px" />

      {/* Zero messages is NOT sufficient. An interaction can be the first
          thing in a session — the agent's opening act can be a write it must
          ask for — and drawing "start a conversation" under a live Allow/Deny
          card invites the user to type at the exact moment they are being
          asked to grant disk access. The gate owns the pane while it is up. */}
      <Show when={messages().length === 0 && !pendingInteraction()}>
        <div
          class="h-full flex flex-col items-center justify-center gap-4"
          data-testid="message-list-empty"
        >
          <Show
            when={session()}
            fallback={
              <>
                <ChatBubbleMark ring="bg-surface-elevated" glyph="text-muted-dark" />
                <p class="max-w-(--cru-measure-empty) text-balance px-4 text-center text-muted-dark">
                  Select or create a session to start chatting
                </p>
              </>
            }
          >
            <>
              <ChatBubbleMark ring="bg-primary/15" glyph="text-primary" />
              {/* `max-w` + `text-balance`: this line used to wrap to one word
                  per line in a narrow pane, which is the worst setting of the
                  first sentence a new user reads. */}
              <p class="max-w-(--cru-measure-empty) text-balance px-4 text-center text-muted">
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
    </div>
  );
};
