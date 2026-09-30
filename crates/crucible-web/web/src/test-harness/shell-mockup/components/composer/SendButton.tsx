/**
 * Send; while a turn runs, send queues the message, and with an empty draft
 * the button stops the turn instead (the current app's cancel).
 */
import { Show, type Component } from 'solid-js';
import { ArrowUp, X } from '@/lib/icons';

export interface SendButtonProps {
  running: boolean;
  empty: boolean;
  onSend: () => void;
  onStop: () => void;
}

export const SendButton: Component<SendButtonProps> = (props) => {
  const stops = () => props.running && props.empty;
  return (
    <button
      type="button"
      class="mk-send"
      classList={{ queue: props.running && !props.empty, stop: stops() }}
      aria-label={stops() ? 'Stop the turn' : props.running ? 'Queue the message' : 'Send'}
      title={stops() ? 'Stop the turn' : props.running ? 'Waits for the turn to end' : 'Send'}
      disabled={props.empty && !props.running}
      onClick={() => (stops() ? props.onStop() : props.onSend())}
    >
      <Show when={stops()} fallback={<ArrowUp class="mk-i" />}>
        <X class="mk-i" />
      </Show>
    </button>
  );
};
