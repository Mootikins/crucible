import { Component, Show, For, createMemo, createSignal, createEffect, onCleanup } from 'solid-js';
import { ToolCard } from './ToolCard';
import { ThinkingBlock } from './ThinkingBlock';
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

interface MessageProps {
  message: MessageType;
  isStreaming?: boolean;
}

export const Message: Component<MessageProps> = (props) => {
  const isUser = () => props.message.role === 'user';
  const isSystem = () => props.message.role === 'system';
  const isPrecognition = () => props.message.role === 'system' && props.message.type === 'precognition';
  const isEmpty = () => !props.message.content || props.message.content.length === 0;
  const hasToolCalls = () => props.message.toolCalls && props.message.toolCalls.length > 0;
  const hasThinking = () => !!props.message.thinking && props.message.thinking.content.length > 0;
  const [renderedContent, setRenderedContent] = createSignal('');
  const [showPrecognitionNotes, setShowPrecognitionNotes] = createSignal(false);
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

  return (
    <div
      class={`mb-4 flex ${isUser() ? 'justify-end' : 'justify-start'}`}
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

           <Show when={props.isStreaming}>
             <span class="inline-block w-2 h-4 bg-primary-hover animate-pulse ml-0.5" />
           </Show>
        </Show>
      </div>
    </div>
  );
};
