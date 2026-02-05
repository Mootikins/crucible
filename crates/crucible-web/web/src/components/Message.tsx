import { Component, Show, For, createMemo } from 'solid-js';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { ToolCard } from './ToolCard';
import type { Message as MessageType } from '@/lib/types';

marked.setOptions({
  breaks: true,
  gfm: true,
});

interface MessageProps {
  message: MessageType;
  isStreaming?: boolean;
}

export const Message: Component<MessageProps> = (props) => {
  const isUser = () => props.message.role === 'user';
  const isEmpty = () => !props.message.content || props.message.content.length === 0;
  const hasToolCalls = () => props.message.toolCalls && props.message.toolCalls.length > 0;

  const renderedContent = createMemo(() => {
    if (isEmpty()) return '';
    if (isUser()) return props.message.content;
    
    try {
      const html = marked.parse(props.message.content) as string;
      return DOMPurify.sanitize(html);
    } catch {
      return DOMPurify.sanitize(props.message.content);
    }
  });

  return (
    <div
      class={`flex ${isUser() ? 'justify-end' : 'justify-start'} mb-4`}
      data-testid={`message-${props.message.id}`}
      data-role={props.message.role}
    >
      <div
        class={`max-w-[80%] rounded-2xl px-4 py-2 ${
          isUser()
            ? 'bg-blue-600 text-white rounded-br-md'
            : 'bg-neutral-800 text-neutral-100 rounded-bl-md'
        }`}
      >
        <Show
          when={!isEmpty() || hasToolCalls()}
          fallback={
            <span class="inline-flex items-center gap-1">
              <span class="w-2 h-2 bg-neutral-500 rounded-full animate-pulse" />
              <span
                class="w-2 h-2 bg-neutral-500 rounded-full animate-pulse"
                style={{ 'animation-delay': '75ms' }}
              />
              <span
                class="w-2 h-2 bg-neutral-500 rounded-full animate-pulse"
                style={{ 'animation-delay': '150ms' }}
              />
            </span>
          }
        >
          <Show when={hasToolCalls()}>
            <div class="mb-2">
              <For each={props.message.toolCalls}>
                {(tool) => <ToolCard tool={tool} />}
              </For>
            </div>
          </Show>

          <Show when={!isEmpty()}>
            <Show
              when={!isUser()}
              fallback={<p class="whitespace-pre-wrap break-words">{props.message.content}</p>}
            >
              <div 
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
          
          <Show when={props.isStreaming}>
            <span class="inline-block w-2 h-4 bg-blue-400 animate-pulse ml-0.5" />
          </Show>
        </Show>
      </div>
    </div>
  );
};
