/** Send, or, while a turn runs, queue the message for the turn's end. */
import type { Component } from 'solid-js';
import { ArrowUp } from 'lucide-solid';

export const SendButton: Component<{ running: boolean; disabled: boolean; onSend: () => void }> = (props) => (
  <button
    type="button"
    class="mk-send"
    classList={{ queue: props.running }}
    aria-label={props.running ? 'Queue the message' : 'Send'}
    title={props.running ? 'Waits for the turn to end' : 'Send'}
    disabled={props.disabled}
    onClick={() => props.onSend()}
  >
    <ArrowUp class="mk-i" />
  </button>
);
