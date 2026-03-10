import { Component, Show, For, createMemo, createSignal, createEffect, onCleanup } from 'solid-js';
import { Copy, Check, Pencil, RefreshCw } from 'lucide-solid';
import { ToolCard } from './ToolCard';
import { ThinkingBlock } from './ThinkingBlock';
import { useChatSafe } from '@/contexts/ChatContext';
import type { Message as MessageType, ToolCallDisplay } from '@/lib/types';
import { renderMarkdown, renderMarkdownAsync } from '@/lib/markdown';

const PRECOGNITION_PATTERN = /^Auto-enriched with (\d+) notes:\s*\[(.*)\]$/s;

function parsePrecognition(content: string): { notesCount: number; notes: string[] } | null {
  const match = PRECOGNITION_PATTERN.exec(content.trim());
  if (!match) {
    return null;
  }

  const notesCount = Number.parseInt(match[1], 10);
  const notes = match[2]
    .split(',')
    .map((note) => note.trim())
    .filter((note) => note.length > 0);

  return {
    notesCount: Number.isNaN(notesCount) ? notes.length : notesCount,
    notes,
  };
}

function addCopyButtons(container: HTMLDivElement): void {
  const blocks = container.querySelectorAll('pre');

  for (const block of blocks) {
    if (block.dataset.copyButton === 'true') {
      continue;
    }

    block.dataset.copyButton = 'true';
    block.classList.add('relative');

    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = 'Copy';
    button.className = 'absolute top-2 right-2 rounded border border-neutral-600 bg-neutral-900/90 px-2 py-1 text-[10px] font-medium text-neutral-200 hover:bg-neutral-800';

    button.addEventListener('click', async (event) => {
      event.preventDefault();
      event.stopPropagation();
      const code = block.querySelector('code')?.textContent ?? '';
      try {
        await navigator.clipboard.writeText(code);
        button.textContent = 'Copied';
        window.setTimeout(() => {
          button.textContent = 'Copy';
        }, 1200);
      } catch {
        button.textContent = 'Failed';
        window.setTimeout(() => {
          button.textContent = 'Copy';
        }, 1200);
      }
    });

    block.append(button);
  }
}

/** Format a timestamp as relative time (e.g., "2 min ago") */
function formatRelativeTime(timestamp: number): string {
  const now = Date.now();
  const diffMs = now - timestamp;
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  if (diffSec < 60) return 'just now';
  if (diffMin < 60) return `${diffMin} min ago`;
  if (diffHour < 24) return `${diffHour} hour${diffHour === 1 ? '' : 's'} ago`;

  if (diffDay === 1) {
    const date = new Date(timestamp);
    const hours = date.getHours().toString().padStart(2, '0');
    const minutes = date.getMinutes().toString().padStart(2, '0');
    return `Yesterday at ${hours}:${minutes}`;
  }

  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

interface MessageProps {
  message: MessageType;
  isStreaming?: boolean;
  isLast?: boolean;
}

export const Message: Component<MessageProps> = (props) => {
  const chat = useChatSafe();
  const isUser = () => props.message.role === 'user';
  const isSystem = () => props.message.role === 'system';
  const isAssistant = () => props.message.role === 'assistant';
  const isPrecognition = () => props.message.role === 'system' && props.message.type === 'precognition';
  const isEmpty = () => !props.message.content || props.message.content.length === 0;
  const hasToolCalls = () => props.message.toolCalls && props.message.toolCalls.length > 0;
  const hasThinking = () => !!props.message.thinking && props.message.thinking.content.length > 0;
  const [renderedContent, setRenderedContent] = createSignal('');
  const [showPrecognitionNotes, setShowPrecognitionNotes] = createSignal(false);
  const [copied, setCopied] = createSignal(false);
  const [isEditing, setIsEditing] = createSignal(false);
  const [editContent, setEditContent] = createSignal('');
  let markdownRef: HTMLDivElement | undefined;

  const toolCalls = createMemo<ToolCallDisplay[]>(() => {
    const calls = props.message.toolCalls ?? [];
    return calls.map((tool) => ({
      id: tool.id,
      callId: tool.id,
      name: tool.title,
      args: '',
      status: 'complete',
    }));
  });

  const precognition = createMemo(() => parsePrecognition(props.message.content));

  createEffect(() => {
    if (isUser() || isPrecognition() || isEmpty()) {
      setRenderedContent('');
      return;
    }

    const content = props.message.content;
    setRenderedContent(renderMarkdown(content));

    let cancelled = false;
    void renderMarkdownAsync(content).then((html) => {
      if (!cancelled) {
        setRenderedContent(html);
      }
    });

    onCleanup(() => {
      cancelled = true;
    });
  });

  createEffect(() => {
    renderedContent();
    if (markdownRef) {
      addCopyButtons(markdownRef);
    }
  });

  const handleRenderedClick = (event: MouseEvent) => {
    const target = event.target as HTMLElement | null;
    const noteElement = target?.closest('[data-note]') as HTMLElement | null;
    if (!noteElement) {
      return;
    }

    event.preventDefault();
    const note = noteElement.dataset.note;
    if (note) {
      console.log(`[wikilink] ${note}`);
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(props.message.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API not available
    }
  };

  const handleEditStart = () => {
    setEditContent(props.message.content);
    setIsEditing(true);
  };

  const handleEditCancel = () => {
    setIsEditing(false);
    setEditContent('');
  };

  const handleEditSave = async () => {
    const content = editContent().trim();
    if (!content) return;
    setIsEditing(false);
    setEditContent('');
    await chat.sendMessage(content);
  };

  const handleRegenerate = async () => {
    const msgs = chat.messages();
    // Find the previous user message before this assistant message
    let prevUserContent: string | null = null;
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === 'user') {
        prevUserContent = msgs[i].content;
        break;
      }
    }
    if (prevUserContent) {
      await chat.sendMessage(prevUserContent);
    }
  };

  const showActions = () => !isSystem() && !isPrecognition() && !props.isStreaming;

  return (
    <div
      class={`group relative mb-4 flex ${isUser() ? 'justify-end' : 'justify-start'}`}
      data-testid={`message-${props.message.role}`}
      data-role={props.message.role}
    >
      <div
        class={
          isUser()
            ? 'message-bubble message-bubble-user'
            : isSystem()
              ? 'max-w-3xl rounded-md border border-neutral-800/60 bg-neutral-900/40 px-3 py-2 text-xs italic text-neutral-400'
              : 'message-assistant'
        }
      >
        <Show
          when={!isEmpty() || hasToolCalls()}
          fallback={
            <span class="inline-flex items-center gap-1">
              <span class="w-2 h-2 bg-muted rounded-full animate-pulse" />
              <span
                class="w-2 h-2 bg-muted rounded-full animate-pulse"
                style={{ 'animation-delay': '75ms' }}
              />
              <span
                class="w-2 h-2 bg-muted rounded-full animate-pulse"
                style={{ 'animation-delay': '150ms' }}
              />
            </span>
          }
        >
          <Show when={hasThinking()}>
            <ThinkingBlock
              content={props.message.thinking!.content}
              isStreaming={props.message.thinking!.isStreaming}
              tokenCount={props.message.thinking!.tokenCount}
            />
          </Show>

          <Show when={hasToolCalls()}>
            <div class="mb-2">
              <For each={toolCalls()}>
                {(tool) => <ToolCard toolCall={tool} />}
              </For>
            </div>
          </Show>

          <Show when={!isEmpty()}>
            <Show when={isPrecognition()}>
              <div class="rounded border border-neutral-700/50 bg-neutral-900/30 px-2 py-1 not-italic text-neutral-300">
                <div class="flex items-center gap-2">
                  <span class="text-xs">Auto-enriched with {precognition()?.notesCount ?? 0} notes</span>
                  <button
                    type="button"
                    class="text-[11px] text-blue-400 hover:text-blue-300"
                    onClick={() => setShowPrecognitionNotes((value) => !value)}
                  >
                    {showPrecognitionNotes() ? 'Hide' : 'Show'} notes
                  </button>
                </div>
                <Show when={showPrecognitionNotes()}>
                  <div class="mt-1 flex flex-wrap gap-1">
                    <For each={precognition()?.notes ?? []}>
                      {(note) => (
                        <span class="rounded border border-neutral-700 bg-neutral-800/80 px-1.5 py-0.5 font-mono text-[11px] not-italic text-neutral-300">
                          {note}
                        </span>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>

            <Show when={!isPrecognition()}>
              <Show when={!isEditing()} fallback={
                <div class="flex flex-col gap-2">
                  <textarea
                    class="w-full rounded border border-neutral-600 bg-neutral-800 px-3 py-2 text-sm text-neutral-200 focus:border-blue-500 focus:outline-none resize-y min-h-[60px]"
                    value={editContent()}
                    onInput={(e) => setEditContent(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Escape') {
                        e.preventDefault();
                        handleEditCancel();
                      }
                    }}
                    ref={(el) => {
                      // Auto-focus and set cursor to end
                      queueMicrotask(() => {
                        el.focus();
                        el.setSelectionRange(el.value.length, el.value.length);
                      });
                    }}
                  />
                  <div class="flex gap-2 justify-end">
                    <button
                      type="button"
                      class="rounded px-3 py-1 text-xs text-neutral-400 hover:text-neutral-200 hover:bg-neutral-700 transition-colors"
                      onClick={handleEditCancel}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      class="rounded bg-blue-600 px-3 py-1 text-xs text-white hover:bg-blue-500 transition-colors"
                      onClick={handleEditSave}
                    >
                      Save & Send
                    </button>
                  </div>
                </div>
              }>
                <Show
                  when={!isUser()}
                  fallback={<p class="whitespace-pre-wrap break-words">{props.message.content}</p>}
                >
                  <div
                    ref={markdownRef}
                    onClick={handleRenderedClick}
                    class="prose prose-invert prose-sm max-w-none
                      prose-p:my-1 prose-p:leading-relaxed
                      prose-pre:bg-neutral-900 prose-pre:rounded-lg prose-pre:p-3 prose-pre:text-sm
                      prose-code:bg-neutral-700 prose-code:px-1 prose-code:rounded prose-code:text-sm prose-code:before:content-none prose-code:after:content-none
                      prose-ul:my-1 prose-ol:my-1 prose-li:my-0.5
                      prose-headings:my-2 prose-headings:font-semibold
                      prose-a:text-blue-400 prose-a:no-underline hover:prose-a:underline
                      prose-blockquote:border-l-2 prose-blockquote:border-neutral-600 prose-blockquote:pl-3 prose-blockquote:italic prose-blockquote:text-neutral-400"
                    innerHTML={renderedContent()}
                  />
                </Show>
              </Show>
            </Show>
          </Show>

           <Show when={props.isStreaming}>
             <span class="inline-block w-2 h-4 bg-primary-hover animate-pulse ml-0.5" />
           </Show>
        </Show>

        {/* Timestamp */}
        <Show when={props.message.timestamp && !isSystem()}>
          <div class={`mt-1 text-xs text-neutral-500 ${isUser() ? 'text-right' : 'text-left'}`}>
            {formatRelativeTime(props.message.timestamp)}
          </div>
        </Show>
      </div>

      {/* Action buttons — visible on hover */}
      <Show when={showActions()}>
        <div
          class={`absolute ${isUser() ? 'right-0 -bottom-6' : 'left-0 -bottom-6'} flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-150`}
        >
          {/* Copy — all messages */}
          <button
            type="button"
            class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
            title={copied() ? 'Copied!' : 'Copy message'}
            onClick={handleCopy}
          >
            <Show when={copied()} fallback={<Copy size={14} />}>
              <Check size={14} class="text-emerald-400" />
            </Show>
          </button>

          {/* Edit — user messages only */}
          <Show when={isUser()}>
            <button
              type="button"
              class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
              title="Edit message"
              onClick={handleEditStart}
            >
              <Pencil size={14} />
            </button>
          </Show>

          {/* Regenerate — last assistant message only */}
          <Show when={isAssistant() && props.isLast}>
            <button
              type="button"
              class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
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
