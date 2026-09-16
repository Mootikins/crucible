import { createSignal } from 'solid-js';

export type RevealState = 'closed' | 'revealed' | 'pinned';

export interface RevealController {
  state: () => RevealState;
  pointerEnter(): void;
  pointerLeave(): void;
  pin(): void;
  unpin(): void;
  tap(): void;
  /** Clear a pending timer. The owner calls it on cleanup. */
  dispose(): void;
}

/**
 * The one timing rule for every hover reveal: a rail's hot zone and a pane's
 * band.
 *
 * - An enter while closed arms the reveal timer.
 * - A leave before that timer fires cancels the reveal.
 * - A leave while revealed arms the close timer.
 * - An enter while revealed cancels a pending close.
 * - A pin cancels any timer and ignores a leave until unpin.
 * - An unpin closes. A tap toggles the pin, for touch where hover does not exist.
 *
 * The state is a Solid signal, so a caller creates the controller inside
 * a reactive root.
 */
export function createRevealController(opts: { enterDelay: number; leaveDelay: number }): RevealController {
  const [state, setState] = createSignal<RevealState>('closed');
  let timer: ReturnType<typeof setTimeout> | null = null;
  const clear = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  };
  const arm = (delay: number, next: RevealState) => {
    timer = setTimeout(() => {
      timer = null;
      setState(next);
    }, delay);
  };
  return {
    state,
    pointerEnter() {
      clear();
      if (state() === 'closed') arm(opts.enterDelay, 'revealed');
    },
    pointerLeave() {
      clear();
      if (state() === 'revealed') arm(opts.leaveDelay, 'closed');
    },
    pin() {
      clear();
      setState('pinned');
    },
    unpin() {
      clear();
      setState('closed');
    },
    tap() {
      clear();
      setState(state() === 'pinned' ? 'closed' : 'pinned');
    },
    dispose: clear,
  };
}
