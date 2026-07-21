/**
 * One assistant TURN — everything the agent did for a single user prompt:
 * interleaved text segments and tool-call groups, rendered as one block with
 * ONE meta row (token usage + timestamp) for the whole response, the way
 * other agent UIs treat a response as a unit. Individual segments carry no
 * chrome of their own.
 *
 * Structure comes in as id lists (not message objects): each part resolves
 * its live message from the store by id, so streaming token appends update
 * fine-grained without any wrapper churn or DOM remounts.
 */
import { Component, For, Show, createMemo, createSignal, createEffect, onCleanup } from 'solid-js';
import { Copy, Check, RefreshCw } from 'lucide-solid';
import { ThinkingBlock } from './ThinkingBlock';
import { ToolCard } from './ToolCard';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { Message as MessageType, TokenUsage } from '@/lib/types';
import { renderMarkdown, renderMarkdownChatAsync, PROSE_CLASS } from '@/lib/markdown';
import { openNoteInEditor } from '@/lib/note-actions';
import { statusBarStore } from '@/stores/statusBarStore';
import { formatRelativeTime } from '@/lib/format-time';

export type TurnPartSpec =
  | { kind: 'text'; id: string }
  | { kind: 'tools'; key: string; ids: string[] };

/** Format token usage as a compact string, e.g. "150 tokens (25 cached)" */
function formatTokenUsage(usage: TokenUsage): string {
  const parts: string[] = [`${usage.totalTokens.toLocaleString()} tokens`];
  const cached = (usage.cacheReadTokens ?? 0) + (usage.cacheCreationTokens ?? 0);
  if (cached > 0) {
    parts.push(`(${cached.toLocaleString()} cached)`);
  }
  return parts.join(' ');
}

const WorkingDots: Component = () => (
  <span class="inline-flex items-center gap-1 py-1" data-testid="working-indicator">
    <span class="w-2 h-2 bg-muted rounded-full animate-pulse" />
    <span class="w-2 h-2 bg-muted rounded-full animate-pulse" style={{ 'animation-delay': '75ms' }} />
    <span class="w-2 h-2 bg-muted rounded-full animate-pulse" style={{ 'animation-delay': '150ms' }} />
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

  return (
    <div data-testid="message-assistant" data-role="assistant">
      <Show when={thinking() && thinking()!.content.length > 0 && statusBarStore.showThinking()}>
        <ThinkingBlock
          content={thinking()!.content}
          isStreaming={thinking()!.isStreaming}
          tokenCount={thinking()!.tokenCount}
        />
      </Show>
      <Show when={content() !== ''} fallback={<Show when={!thinking()?.isStreaming}><WorkingDots /></Show>}>
        <div class={PROSE_CLASS} onClick={props.onMarkdownClick} innerHTML={renderedContent()} />
      </Show>
      <Show when={props.showCaret && content() !== ''}>
        <span class="inline-block w-2 h-4 bg-primary-hover animate-pulse ml-0.5" />
      </Show>
    </div>
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

  const sessionKiln = () => {
    const sid = chat.sessionId?.();
    return sessionCtx.sessions().find((s) => s.id === sid)?.kiln;
  };

  // Shared with the note reading view's click semantics: md-codeblock copy
  // buttons copy, wikilinks open notes, external links open a new tab,
  // relative links are kiln note references.
  const handleMarkdownClick = (event: MouseEvent) => {
    const target = event.target as HTMLElement | null;

    const copyBtn = target?.closest?.('[data-copy]');
    if (copyBtn) {
      event.preventDefault();
      const pre = copyBtn.closest('.md-codeblock')?.querySelector('pre');
      const code = pre?.textContent ?? '';
      if (code) {
        void navigator.clipboard?.writeText(code);
        const prev = copyBtn.textContent;
        copyBtn.textContent = 'Copied';
        copyBtn.classList.add('is-copied');
        setTimeout(() => {
          copyBtn.textContent = prev;
          copyBtn.classList.remove('is-copied');
        }, 1200);
      }
      return;
    }

    const noteElement = target?.closest('[data-note]') as HTMLElement | null;
    if (noteElement) {
      event.preventDefault();
      const note = noteElement.dataset.note;
      if (note) void openNoteInEditor(note, sessionKiln());
      return;
    }

    const anchor = target?.closest('a') as HTMLAnchorElement | null;
    if (!anchor) return;
    const href = anchor.getAttribute('href') ?? '';
    if (!href || href.startsWith('#')) return;
    event.preventDefault();
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
      window.open(href, '_blank', 'noopener,noreferrer');
      return;
    }
    const note = decodeURIComponent(href)
      .replace(/^\.?\//, '')
      .replace(/\.md$/i, '');
    void openNoteInEditor(note, sessionKiln());
  };

  // Turn-level meta: ONE timestamp (turn start) and ONE usage line (whichever
  // part carries it — the daemon attaches usage to the turn's final segment).
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
      window.setTimeout(() => setCopied(false), 2000);
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
    <div class="group relative mb-6" data-testid="assistant-turn" data-role="assistant-turn">
      <div class="flex flex-col gap-1.5">
        <For each={props.parts}>
          {(part) => {
            if (part.kind === 'tools') {
              return (
                <div class="flex justify-start" data-role="tool">
                  <div
                    class="w-full border border-hairline rounded-md overflow-hidden divide-y divide-hairline bg-surface-base"
                    data-testid="tool-group"
                  >
                    <For each={part.ids}>
                      {(id) => {
                        const tool = createMemo(() => byId(id)?.toolCall);
                        return (
                          <Show when={tool()}>
                            <ToolCard toolCall={tool()!} grouped />
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

      {/* ONE meta row for the whole response — never per segment. Tiny and
          muted, out of the reading flow: usage · time, hairline-quiet. */}
      <Show when={!turnInFlight()}>
        <div class="mt-2 flex items-center gap-1.5 text-[11px] leading-none text-muted-dark">
          <Show when={usage()}>
            <span>{formatTokenUsage(usage()!)}</span>
          </Show>
          <Show when={usage() && firstMessage()?.timestamp}>
            <span aria-hidden="true">·</span>
          </Show>
          <Show when={firstMessage()?.timestamp}>
            <span data-dynamic-time>{formatRelativeTime(firstMessage()!.timestamp)}</span>
          </Show>
        </div>
      </Show>

      {/* Hover actions for the whole turn */}
      <Show when={!turnInFlight()}>
        <div class="absolute left-0 -bottom-5 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
          <button
            type="button"
            class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
            title={copied() ? 'Copied!' : 'Copy response'}
            onClick={handleCopy}
          >
            <Show when={copied()} fallback={<Copy size={14} />}>
              <Check size={14} class="text-ok" />
            </Show>
          </button>
          <Show when={props.isLast}>
            <button
              type="button"
              class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
              title="Regenerate response"
              onClick={handleRegenerate}
            >
              <RefreshCw size={14} />
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
};
