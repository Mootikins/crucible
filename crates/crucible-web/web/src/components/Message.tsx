/**
 * A USER or SYSTEM transcript row. Assistant output never renders here —
 * MessageList groups an entire assistant response (text segments + tool
 * runs) into one AssistantTurn block with a single meta row.
 */
import { makeMarkdownClickHandler } from '@/lib/markdown-click';
import { Component, Show, createSignal } from 'solid-js';
import { Copy, Check, Pencil } from 'lucide-solid';
import { PrecognitionBadge } from './PrecognitionBadge';
import { TurnGutter } from './TurnGutter';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { sessionDefaultKiln } from '@/lib/session-scope';
import { kilnPathOf } from '@/stores/kilnStore';
import type { Message as MessageType } from '@/lib/types';
import { renderPlainWithWikilinks } from '@/lib/markdown';
import { formatMessageTime } from '@/lib/format-time';

interface MessageProps {
  message: MessageType;
}

export const Message: Component<MessageProps> = (props) => {
  const chat = useChatSafe();
  const sessionCtx = useSessionSafe();

  /**
   * The kiln a transcript's links belong to is the SESSION's kiln — canonical
   * state from the session record, not whichever kiln the navigator is showing.
   * Switching kilns must not re-point the links in an open conversation.
   *
   * Resolved name → directory, because wikilink resolution takes a root.
   */
  const sessionKiln = () => {
    const sid = chat.sessionId?.();
    const s = sessionCtx.sessions().find((x) => x.id === sid);
    return (s ? kilnPathOf(sessionDefaultKiln(s)) : null) ?? undefined;
  };
  const handleMarkdownClick = makeMarkdownClickHandler();
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
      window.setTimeout(() => setCopied(false), 1200);
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
      // The row is content + gutter. `items-start` puts the gutter at the top
      // of the turn; the gap between rows belongs to the list, not here.
      class="group relative flex items-start gap-1 justify-start"
      data-testid={`message-${props.message.role}`}
      data-role={props.message.role}
    >
      <div class="min-w-0 flex-1">
      <div
        class={
          isUser()
            ? // The bubble sizes to its text (`.user-quote`) — except while
              // the editor is open, where a fit-content box would collapse
              // around a textarea's intrinsic width.
              `user-quote${isEditing() ? ' w-full' : ''}`
            : 'w-full rounded-md border border-hairline bg-surface-base px-3 py-1.5 text-reading italic text-muted'
        }
      >
        <Show when={!isEditing()} fallback={
          <div class="flex flex-col gap-2">
            <textarea
              class="w-full rounded border border-hairline bg-control px-3 py-2 text-sm text-shell-ink focus:border-primary focus-ring resize-y min-h-[60px]"
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
                class="rounded bg-primary px-3 py-1 text-xs text-on-primary hover:bg-primary-hover transition-colors"
                onClick={handleEditSave}
                title="Sends the edited text as a new message (history is immutable)"
              >
                Send as new
              </button>
            </div>
          </div>
        }>
          <p
            class="whitespace-pre-wrap break-words"
            // A user bubble renders wikilinks exactly like an assistant turn,
            // so it needs the same handler and the same kiln declaration. It
            // had neither: hovering a link worked (the controller is global)
            // while clicking it did nothing at all.
            data-kiln={sessionKiln() || undefined}
            onClick={handleMarkdownClick}
          >
            <span innerHTML={renderPlainWithWikilinks(props.message.content)} />

            {/* The time the message was sent, INLINE at the end of the last
                line — the trailer a chat app puts there, not a caption on a
                row of its own.

                It is always in the document and only its opacity answers the
                hover, which is the whole no-reflow rule: a stamp that ENTERS
                the flow on hover re-wraps the last line under the pointer,
                and a stamp taken OUT of the flow needs reserved padding to
                sit in, which is the dead space this pass removes. Reserving
                the room inline costs the tail of one line and no height at
                all. The date appears once the message is not from today. */}
            <Show when={isUser() && props.message.timestamp}>
              <span
                class="ml-2 align-baseline text-floor leading-none text-muted-dark opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100"
                data-testid="message-time"
                title={new Date(props.message.timestamp).toLocaleString()}
              >
                {formatMessageTime(props.message.timestamp)}
              </span>
            </Show>
          </p>
        </Show>
        <Show when={isUser() && hasPrecognition()}>
          <PrecognitionBadge
            notesCount={props.message.precognition!.notesCount}
            notes={props.message.precognition!.notes}
          />
        </Show>
      </div>
      </div>

      {/* The gutter is drawn for a system row too, empty: it is what holds
          every row's reading edge on the same line. */}
      <TurnGutter>
        <Show when={!isSystem()}>
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
        </Show>
      </TurnGutter>
    </div>
  );
};
