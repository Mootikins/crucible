import { Component, JSX, Show } from 'solid-js';
import { ChevronRight } from '@/lib/icons';

/**
 * A collapsible section of the sessions rail: Inbox, Projects, Archived.
 *
 * One row at the rail's row height, and one chevron slot, so a section header
 * and the rows under it share one leading edge. A section with nothing in it
 * is not offered, unless `always` says its actions must stay reachable.
 */
export const TreeSection: Component<{
  label: string;
  count: number;
  open: boolean;
  onToggle: () => void;
  testid: string;
  /** Draw the count in the session accent — the Inbox uses it for "waiting". */
  urgent?: boolean;
  /** Keep the header when the count is zero: its `actions` still apply. */
  always?: boolean;
  /** Controls on the header row, between the label and the count: they must not nest in the toggle. */
  actions?: JSX.Element;
  children: JSX.Element;
}> = (props) => (
  <Show when={props.count > 0 || props.always}>
    <div>
      <div class="flex items-center">
        <button
          type="button"
          data-testid={props.testid}
          aria-expanded={props.count > 0 ? props.open : undefined}
          onClick={() => props.count > 0 && props.onToggle()}
          class="flex-1 min-w-0 flex items-center gap-2 px-2 h-(--cru-row-sm) text-floor leading-4 font-semibold uppercase tracking-wide text-muted-dark hover:text-shell-body"
        >
          <Show
            when={props.count > 0}
            fallback={<span class="w-3.5 h-3.5 shrink-0 inline-block" aria-hidden="true" />}
          >
            <ChevronRight class={`w-3.5 h-3.5 shrink-0 transition-transform ${props.open ? 'rotate-90' : ''}`} />
          </Show>
          <span class="flex-1 text-left truncate">{props.label}</span>
          <Show when={props.count > 0}>
            <span class="sr-only">{props.count}</span>
          </Show>
        </button>
        <Show when={props.actions}>
          <div class="shrink-0 px-1">{props.actions}</div>
        </Show>
        <Show when={props.count > 0}>
          <span
            class="tabular-nums pr-2 cursor-pointer text-floor leading-4 font-semibold tracking-wide"
            // One colour class at a time: two on one element resolve by
            // stylesheet order, not by intent.
            classList={{
              'text-attention': props.urgent === true,
              'text-muted-dark hover:text-shell-body': props.urgent !== true,
            }}
            aria-hidden="true"
            onClick={() => props.onToggle()}
          >
            {props.count}
          </span>
        </Show>
      </div>
      <Show when={props.open && props.count > 0}>{props.children}</Show>
    </div>
  </Show>
);
