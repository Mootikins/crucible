import { Component } from 'solid-js';
import type { SessionStatus } from '@/lib/session-status';

const LABEL: Record<SessionStatus, string> = {
  working: 'Working',
  waiting: 'Waiting for you',
  idle: 'Idle',
};

/**
 * Seven pixels that answer "does this need me?".
 *
 * Filled green — the machine is busy.
 * Hollow amber — YOU are the blocker.
 * Grey         — nothing is happening.
 *
 * FILL against RING first, hue second. The fill/ring split has to survive at
 * this size and on a colour-blind reader's screen, so it carries the meaning
 * on its own; the hues then reinforce it with the vocabulary the rest of the
 * app already speaks — `attention` is "waiting on you" in the Inbox, the
 * Changes panel and the status chips, and `ok` is a live stream.
 *
 * Sized and bordered inline rather than by class: a 1.5px ring has no Tailwind
 * width, and rounding it to 1px or 2px is what makes a hollow dot read as a
 * filled one at small sizes.
 */
export const SessionStatusDot: Component<{
  status: SessionStatus;
  /**
   * Name the state to assistive tech. OFF by default: beside a row that
   * already has a title the dot is decoration, and a 200-row session list
   * would otherwise read "Idle, image" 200 times and pop a tooltip on every
   * hover. Turn it on where the dot is the ONLY carrier of the state.
   */
  labelled?: boolean;
  class?: string;
}> = (props) => {
  // 7px, fixed. A size knob promised "the ring stays 1.5px at every size" — a
  // contract no caller exercised and nothing tested.
  const size = () => 7;
  const fill = () =>
    props.status === 'working'
      ? 'var(--color-ok)'
      : props.status === 'idle'
        ? 'var(--color-muted)'
        : 'transparent';

  return (
    <span
      role={props.labelled ? 'img' : undefined}
      aria-hidden={props.labelled ? undefined : 'true'}
      data-testid="session-status-dot"
      data-status={props.status}
      aria-label={props.labelled ? LABEL[props.status] : undefined}
      title={props.labelled ? LABEL[props.status] : undefined}
      class={`inline-block shrink-0 rounded-full ${props.class ?? ''}`}
      style={{
        width: `${size()}px`,
        height: `${size()}px`,
        'box-sizing': 'border-box',
        background: fill(),
        // Longhands, not the `border` shorthand: a shorthand whose value holds
        // a var() is stored unexpanded, so `borderStyle` reads back empty and
        // nothing can assert the ring.
        'border-width': '1.5px',
        'border-style': props.status === 'waiting' ? 'solid' : 'none',
        'border-color': 'var(--color-attention)',
      }}
    />
  );
};
