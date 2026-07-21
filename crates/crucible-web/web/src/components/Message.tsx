/**
 * A USER or SYSTEM transcript row. Assistant output never renders here —
 * MessageList groups an entire assistant response (text segments + tool
 * runs) into one AssistantTurn block with a single meta row.
 */
import { Component, Show, createSignal } from 'solid-js';
import { Copy, Check, Pencil } from 'lucide-solid';
import { PrecognitionBadge } from './PrecognitionBadge';
import { useChatSafe } from '@/contexts/ChatContext';
import type { Message as MessageType } from '@/lib/types';
import { renderPlainWithWikilinks } from '@/lib/markdown';
import { formatRelativeTime } from '@/lib/format-time';

interface MessageProps {
  message: MessageType;
}

export const Message: Component<MessageProps> = (props) => {
  const chat = useChatSafe();
  const isUser = () => props.message.role === 'user';
  const isSystem = () => props.message.role === 'system';
  const hasPrecognition = () => !!props.message.precognition;
  const [copied, setCopied] = createSignal(false);
  const [isEditing, setIsEditing] = createSignal(false);
  const [editContent, setEditContent] = createSignal('');

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

  return (
    <div
      class={`group relative mb-5 flex ${isUser() ? 'justify-end' : 'justify-start'}`}
      data-testid={`message-${props.message.role}`}
      data-role={props.message.role}
    >
      <div
        class={
          isUser()
            ? 'message-bubble message-bubble-user'
            : 'rounded-md border border-hairline bg-surface-base px-3 py-2 text-xs italic text-muted'
        }
      >
        <Show when={!isEditing()} fallback={
          <div class="flex flex-col gap-2">
            <textarea
              class="w-full rounded border border-hairline bg-control px-3 py-2 text-sm text-shell-ink focus:border-primary focus:outline-none resize-y min-h-[60px]"
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
                class="rounded px-3 py-1 text-xs text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors"
                onClick={handleEditCancel}
              >
                Cancel
              </button>
              <button
                type="button"
                class="rounded bg-primary px-3 py-1 text-xs text-white hover:bg-primary-hover transition-colors"
                onClick={handleEditSave}
              >
                Save & Send
              </button>
            </div>
          </div>
        }>
          <p
            class="whitespace-pre-wrap break-words"
            innerHTML={renderPlainWithWikilinks(props.message.content)}
          />
        </Show>
        <Show when={isUser() && hasPrecognition()}>
          <PrecognitionBadge
            notesCount={props.message.precognition!.notesCount}
            notes={props.message.precognition!.notes}
          />
        </Show>

        <Show when={isUser() && props.message.timestamp}>
          <div class="mt-1 text-right text-xs text-muted-dark">
            {formatRelativeTime(props.message.timestamp)}
          </div>
        </Show>
      </div>

      {/* Hover actions */}
      <Show when={!isSystem()}>
        <div class="absolute right-0 -bottom-5 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
          <button
            type="button"
            class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
            title={copied() ? 'Copied!' : 'Copy message'}
            onClick={handleCopy}
          >
            <Show when={copied()} fallback={<Copy size={14} />}>
              <Check size={14} class="text-ok" />
            </Show>
          </button>
          <Show when={isUser()}>
            <button
              type="button"
              class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
              title="Edit message"
              onClick={handleEditStart}
            >
              <Pencil size={14} />
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
};
