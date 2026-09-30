/**
 * Reject, then Accept: the pair that every review surface shows for a hunk
 * or for a whole review. In the real app the pair calls
 * `reviewActions.setState` (accept) and `reviewActions.reject`.
 */
import { Show, createSignal, type Component } from 'solid-js';
import { Check } from '@/lib/icons';
import { Button } from './Button';

export interface DecisionButtonsProps {
  onAccept: () => void;
  onReject: () => void;
  acceptLabel?: string;
  rejectLabel?: string;
  /** The accept button takes the primary fill. */
  primary?: boolean;
  /** A check mark before the accept label. The default is on. */
  check?: boolean;
  disabled?: boolean;
  /**
   * A reject reverts on disk. With a value, the first click only arms the
   * button and shows this label; a second click inside 3.5 s rejects.
   */
  confirmReject?: string;
}

export const DecisionButtons: Component<DecisionButtonsProps> = (props) => {
  const [armed, setArmed] = createSignal(false);
  const reject = () => {
    if (props.confirmReject && !armed()) {
      setArmed(true);
      setTimeout(() => setArmed(false), 3500);
      return;
    }
    props.onReject();
  };
  return (
    <>
      <Button variant="ghost" danger disabled={props.disabled} onClick={reject}>
        {armed() ? props.confirmReject : props.rejectLabel ?? 'Reject'}
      </Button>
      <Button variant={props.primary ? 'primary' : undefined} disabled={props.disabled} onClick={() => props.onAccept()}>
        <Show when={props.check !== false}>
          <Check class="mk-i" />
        </Show>
        {props.acceptLabel ?? 'Accept'}
      </Button>
    </>
  );
};
