/**
 * The message box, in the current app's form: a capsule with the draft,
 * dictation and send, and a quiet row of session chips under it. Enter
 * sends; Shift+Enter makes a new line. The capsule is round while the draft
 * is one line, and takes the card radius when it wraps.
 */
import { createEffect, createSignal, on, type Component } from 'solid-js';
import { Mic } from 'lucide-solid';
import { IconButton } from '../primitives/IconButton';
import { SendButton } from './SendButton';
import { SessionChips, type SessionChipsProps } from './SessionChips';

export interface ComposerProps extends SessionChipsProps {
  draft: string;
  onDraft: (text: string) => void;
  onSend: () => void;
  /** Stops the turn that runs. */
  onStop: () => void;
  /** A turn runs: send queues the message, and an empty draft offers stop. */
  running: boolean;
}

const MAX_HEIGHT_PX = 200;

export const Composer: Component<ComposerProps> = (props) => {
  const [lines, setLines] = createSignal(1);
  const resize = (el: HTMLTextAreaElement) => {
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT_PX)}px`;
    const lh = parseFloat(getComputedStyle(el).lineHeight) || 20;
    setLines(Math.max(1, Math.round(el.scrollHeight / lh)));
  };
  let box: HTMLTextAreaElement | undefined;
  // A draft can change from outside (edit a message): the box fits it too.
  createEffect(on(() => props.draft, () => box && resize(box), { defer: true }));
  return (
    <div class="mk-composer-a">
      <div class="mk-composer" data-lines={lines() > 1 ? 'many' : 'one'}>
        <textarea
          rows={1}
          placeholder="Type a message…"
          aria-label="Message"
          value={props.draft}
          ref={(el) => {
            box = el;
            queueMicrotask(() => resize(el));
          }}
          onInput={(e) => {
            resize(e.currentTarget);
            props.onDraft(e.currentTarget.value);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
              e.preventDefault();
              props.onSend();
            }
          }}
        />
        {/* The real app records through WhisperContext. */}
        <IconButton label="Hold to dictate">
          <Mic class="mk-i" />
        </IconButton>
        <SendButton running={props.running} empty={!props.draft.trim()} onSend={props.onSend} onStop={props.onStop} />
      </div>
      <SessionChips mode={props.mode} model={props.model} workspace={props.workspace} kiln={props.kiln} />
    </div>
  );
};
