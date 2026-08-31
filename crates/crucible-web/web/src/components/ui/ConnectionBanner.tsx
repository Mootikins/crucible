/**
 * One fault line, one recovery control.
 *
 * Three surfaces lose their link to the daemon: the terminal socket, the chat
 * event stream and a file save. Each named the fault in its own words and only
 * ONE of them — the terminal — offered a way out, so the same class of failure
 * taught the user three different lessons about whether it was recoverable.
 *
 * The terminal's affordance is the model here: say what broke, and put the
 * retry inside the thing that says it.
 *
 * `onRetry` is optional ON PURPOSE. A surface that cannot re-issue the failed
 * call must not grow a button that pretends it can; it passes the message
 * alone and the banner renders as a statement.
 */
import { Component, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { AlertTriangle, RefreshCw } from '@/lib/icons';

type ConnectionBannerTone =
  /** Self-healing. A backoff timer is already running; retry only skips the
   *  wait. Neutral, because nothing is lost yet. */
  | 'transient'
  /** An operation failed and stays failed until someone acts. */
  | 'error';

export interface ConnectionBannerProps {
  /** What broke, in one line. */
  message: string;
  tone: ConnectionBannerTone;
  /** Omit to render the fault with no control — see the note above. */
  onRetry?: () => void;
  /** The verb on the control. Required whenever `onRetry` is given. */
  retryLabel?: string;
  /** Positioning is the caller's: the terminal centres this over its canvas,
   *  the composer and the editor stack it in the flow. */
  class?: string;
  testid?: string;
  retryTestid?: string;
}

const TONE: Record<ConnectionBannerTone, string> = {
  transient: 'border-hairline-strong bg-control text-shell-ink',
  error: 'border-error/40 bg-error/10 text-error',
};

const RETRY_TONE: Record<ConnectionBannerTone, string> = {
  transient: 'border-hairline-strong bg-surface-elevated text-shell-ink hover:bg-hover-wash',
  error: 'border-error/50 bg-error/15 text-error hover:bg-error/25',
};

export const ConnectionBanner: Component<ConnectionBannerProps> = (props) => {
  return (
    <div
      // `status`, not `alert`: a dropped socket that reconnects on its own must
      // not interrupt a screen reader mid-sentence.
      role="status"
      aria-live="polite"
      data-testid={props.testid}
      data-tone={props.tone}
      class={`flex items-center gap-2 rounded border px-3 py-1.5 text-sm ${TONE[props.tone]} ${props.class ?? ''}`}
    >
      <Dynamic
        component={props.tone === 'error' ? AlertTriangle : RefreshCw}
        class="w-4 h-4 shrink-0"
        aria-hidden="true"
      />
      <span class="flex-1 min-w-0">{props.message}</span>
      <Show when={props.onRetry}>
        <button
          type="button"
          data-testid={props.retryTestid}
          onClick={() => props.onRetry?.()}
          class={`focus-ring shrink-0 rounded border px-2 py-0.5 text-xs transition-colors ${RETRY_TONE[props.tone]}`}
        >
          {props.retryLabel ?? 'Retry'}
        </button>
      </Show>
    </div>
  );
};
