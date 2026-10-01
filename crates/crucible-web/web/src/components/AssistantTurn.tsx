/**
 * One assistant TURN — everything the agent did for a single user prompt:
 * interleaved text segments and tool-call groups, rendered as one block with
 * ONE meta row for the whole response: individual segments carry no chrome
 * of their own, and the turn's actions and its two measurements share that
 * one row at the bottom of the turn (`TurnMeta`).
 *
 * Structure comes in as id lists (not message objects): each part resolves
 * its live message from the store by id, so streaming token appends update
 * fine-grained without any wrapper churn or DOM remounts.
 */
import { Component, For, Show, createMemo, createSignal, createEffect, onCleanup } from 'solid-js';
import { sessionDefaultKiln } from '@/lib/session-scope';
import { kilnPathOf } from '@/stores/kilnStore';
import { Copy, Check, RefreshCw } from '@/lib/icons';
import { ThinkingBlock } from './ThinkingBlock';
import { ToolCard } from './ToolCard';
import { TurnMeta, AuthorHeading, revealTurnActions } from './TurnMeta';
import { IconButton } from './ui/IconButton';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { Message as MessageType, TokenUsage } from '@/lib/types';
import { renderMarkdown, renderMarkdownChatAsync, proseClass } from '@/lib/markdown';
import { makeMarkdownClickHandler } from '@/lib/markdown-click';
import { statusBarStore } from '@/stores/statusBarStore';
import { formatAbsoluteTime } from '@/lib/format-time';

export type TurnPartSpec =
  | { kind: 'text'; id: string }
  | { kind: 'tools'; key: string; ids: string[] };

/** Format token usage as a compact string, e.g. "150 tokens (25 cached)" */
function formatTokenUsage(usage: TokenUsage): string {
  const total = `${usage.totalTokens.toLocaleString()} tokens`;
  // The row names the cache only when the provider read from it.
  return usage.cacheReadTokens ? `${total} (${usage.cacheReadTokens.toLocaleString()} cached)` : total;
}

// Also shown by DraftSessionPanel's instant pending-preview — one dots
// implementation so the draft handoff and in-turn states can't drift apart.
export const WorkingDots: Component = () => (
  <span class="inline-flex items-center gap-1 py-1" data-testid="working-indicator">
    {/* 160ms apart on a 1400ms cycle — a ninth of the period, which is the
        point a stagger stops being a rounding error and starts reading as a
        wave travelling left to right. See `cru-think` in index.css. */}
    <span class="cru-think-dot h-1.5 w-1.5 rounded-full bg-muted" />
    <span class="cru-think-dot h-1.5 w-1.5 rounded-full bg-muted" style={{ 'animation-delay': '160ms' }} />
    <span class="cru-think-dot h-1.5 w-1.5 rounded-full bg-muted" style={{ 'animation-delay': '320ms' }} />
  </span>
);

const TextSegment: Component<{
  id: string;
  showCaret: boolean;
  onMarkdownClick: (event: MouseEvent) => void;
}> = (props) => {
  const chat = useChatSafe();
  const message = createMemo(() => chat.messages().find((m) => m.id === props.id));
  const content = () => message()?.content ?? '';
  const [renderedContent, setRenderedContent] = createSignal('');

  createEffect(() => {
    const text = content();
    if (!text) {
      setRenderedContent('');
      return;
    }
    // Sync render for immediacy, async pass upgrades code highlighting.
    setRenderedContent(renderMarkdown(text));
    let cancelled = false;
    void renderMarkdownChatAsync(text).then((html) => {
      if (!cancelled) setRenderedContent(html);
    });
    onCleanup(() => {
      cancelled = true;
    });
  });

  const thinking = () => message()?.thinking;

  // A textless segment that can still grow means "text is on its way" —
  // hence the dots. The daemon closes a segment that ends with no text (the
  // model reasoned and went straight to a tool), and a closed segment draws
  // no dots. While the reasoning streams, the thinking block shows it.
  const awaitingText = () => message()?.streaming === true && !thinking()?.isStreaming;
  // A closed segment with no text and no reasoning only carries the usage of
  // the turn (the answer ended at a tool). It draws nothing.
  const drawn = () =>
    content() !== '' || (thinking()?.content ?? '') !== '' || message()?.streaming === true;

  return (
    <Show when={drawn()}>
    <div data-testid="message-assistant" data-role="assistant">
      <Show when={thinking() && thinking()!.content.length > 0 && statusBarStore.showThinking()}>
        <ThinkingBlock
          content={thinking()!.content}
          isStreaming={thinking()!.isStreaming}
          tokenCount={thinking()!.tokenCount}
        />
      </Show>
      <Show
        when={content() !== ''}
        fallback={
          <Show when={awaitingText()}>
            <WorkingDots />
          </Show>
        }
      >
        <div class={proseClass()} onClick={props.onMarkdownClick} innerHTML={renderedContent()} />
      </Show>
      <Show when={props.showCaret && content() !== ''}>
        {/* Sized to the reading text it trails, not to a fixed 16px: the caret
            has to sit on the same baseline as the last glyph the agent wrote,
            and the reading size is a token that can move. */}
        <span
          class="cru-caret ml-0.5 inline-block h-[1.05em] w-[2px] translate-y-[0.15em] bg-primary"
          data-testid="stream-caret"
        />
      </Show>
    </div>
    </Show>
  );
};

export const AssistantTurn: Component<{
  parts: TurnPartSpec[];
  isLast: boolean;
}> = (props) => {
  const chat = useChatSafe();
  const sessionCtx = useSessionSafe();
  const [copied, setCopied] = createSignal(false);

  const byId = (id: string) => chat.messages().find((m) => m.id === id);

  // The session's kiln as a DIRECTORY — wikilink resolution takes a root,
  // while the session record carries a registry name.
  const sessionKiln = () => {
    const sid = chat.sessionId?.();
    const s = sessionCtx.sessions().find((x) => x.session_id === sid);
    return (s ? kilnPathOf(sessionDefaultKiln(s)) : null) ?? undefined;
  };

  // One click-delegation implementation shared with the note reading view
  // (lib/markdown-click.ts) — chat and notes must keep identical semantics.
  const handleMarkdownClick = makeMarkdownClickHandler();

  // Turn-level meta: ONE timestamp (turn start) and ONE usage line (whichever
  // part carries it — the daemon attaches usage to the turn's final segment).
  // A transcript item carries no time, so a turn draws a time only when
  // its first part has one.
  const firstMessage = createMemo<MessageType | undefined>(() => {
    for (const part of props.parts) {
      const id = part.kind === 'text' ? part.id : part.ids[0];
      const m = byId(id);
      if (m) return m;
    }
    return undefined;
  });
  const usage = createMemo<TokenUsage | undefined>(() => {
    for (let i = props.parts.length - 1; i >= 0; i--) {
      const part = props.parts[i];
      if (part.kind !== 'text') continue;
      const u = byId(part.id)?.usage;
      if (u) return u;
    }
    return undefined;
  });

  const lastTextId = () => {
    for (let i = props.parts.length - 1; i >= 0; i--) {
      const part = props.parts[i];
      if (part.kind === 'text') return part.id;
    }
    return undefined;
  };

  const turnInFlight = () => chat.isStreaming() && props.isLast;

  const endsWithEmptyText = () => {
    const last = props.parts[props.parts.length - 1];
    return last?.kind === 'text' && (byId(last.id)?.content ?? '') === '';
  };

  const fullText = () =>
    props.parts
      .filter((p): p is Extract<TurnPartSpec, { kind: 'text' }> => p.kind === 'text')
      .map((p) => byId(p.id)?.content ?? '')
      .filter((c) => c !== '')
      .join('\n\n');

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(fullText());
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard API not available
    }
  };

  const handleRegenerate = async () => {
    const msgs = chat.messages();
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === 'user') {
        await chat.sendMessage(msgs[i].content);
        return;
      }
    }
  };

  return (
    <div
      // Text then meta row, the same column a prompt draws. The turn reserves
      // no room of its own: the footer that `pb-5` and `mb-6` held open is a
      // sibling in the flow now, and the list owns the gap between turns.
      class="group"
      data-testid="assistant-turn"
      data-role="assistant-turn"
      tabIndex={0}
      onPointerUp={revealTurnActions}
      // Same kiln the click handler uses, declared for the document-level
      // hover popovers: a session's transcript belongs to the session's kiln,
      // which is not necessarily the one the status bar points at.
      data-kiln={sessionKiln() || undefined}
    >
      <AuthorHeading>Assistant</AuthorHeading>
      <div class="flex flex-col gap-0.5">
        <For each={props.parts}>
          {(part) => {
            if (part.kind === 'tools') {
              return (
                <div class="flex justify-start" data-role="tool">
                  <div
                    class="w-full overflow-hidden"
                    data-testid="tool-group"
                  >
                    <For each={part.ids}>
                      {(id) => {
                        const tool = createMemo(() => byId(id)?.toolCall);
                        return (
                          <Show when={tool()}>
                            <ToolCard toolCall={tool()!} />
                          </Show>
                        );
                      }}
                    </For>
                  </div>
                </div>
              );
            }
            return (
              <TextSegment
                id={part.id}
                showCaret={turnInFlight() && part.id === lastTextId()}
                onMarkdownClick={handleMarkdownClick}
              />
            );
          }}
        </For>

        {/* Tools are running (or the next segment hasn't started): the turn
            is in flight but no empty text segment exists to carry the dots. */}
        <Show when={turnInFlight() && !endsWithEmptyText()}>
          <WorkingDots />
        </Show>
      </div>

      {/* One meta row for the whole response; the last turn keeps it on. */}
      <Show when={!turnInFlight()}>
        <div class="mt-0.5">
          <TurnMeta always={props.isLast}>
            {/* Read outward from the text: what you can DO with the turn,
                then what the turn cost. */}
            <div class="flex items-center gap-0.5">
              <IconButton
                size="sm"
                title={copied() ? 'Copied!' : 'Copy response'}
                aria-label="Copy response"
                onClick={handleCopy}
              >
                <Show when={copied()} fallback={<Copy class="w-4 h-4" />}>
                  <Check class="w-4 h-4 text-ok" />
                </Show>
              </IconButton>
              <Show when={props.isLast}>
                <IconButton
                  size="sm"
                  title="Regenerate response"
                  aria-label="Regenerate response"
                  onClick={handleRegenerate}
                >
                  <RefreshCw class="w-4 h-4" />
                </IconButton>
              </Show>
            </div>
            {/* The clock time of the turn, when its first part has one. The
                tooltip carries the full date. */}
            <Show when={firstMessage()?.timestamp}>
              <span title={new Date(firstMessage()!.timestamp).toLocaleString()}>
                {formatAbsoluteTime(firstMessage()!.timestamp)}
              </span>
            </Show>
            <Show when={usage()}>
              <span>{formatTokenUsage(usage()!)}</span>
            </Show>
          </TurnMeta>
        </div>
      </Show>
    </div>
  );
};
