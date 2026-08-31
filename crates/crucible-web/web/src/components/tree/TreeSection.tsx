import { Component, JSX, Show } from 'solid-js';
import { ChevronRight } from '@/lib/icons';

/**
 * A collapsible section of the sessions rail: Inbox, No sessions, Archived.
 *
 * ONE component because there were three headers with three different
 * paddings, two different count placements and two different chevron
 * treatments — three sections of one list that did not look like siblings.
 * A section that renders nothing when empty is not offered at all: a control
 * that does nothing must not take a row.
 */
export const TreeSection: Component<{
  label: string;
  count: number;
  open: boolean;
  onToggle: () => void;
  testid: string;
  /** Draw the count in the session accent — the Inbox uses it for "waiting". */
  urgent?: boolean;
  children: JSX.Element;
}> = (props) => (
  <Show when={props.count > 0}>
    <button
      type="button"
      data-testid={props.testid}
      aria-expanded={props.open}
      onClick={props.onToggle}
      class="w-full flex items-center gap-1 px-2 pt-3 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-dark hover:text-shell-body"
    >
      <ChevronRight class={`w-3 h-3 shrink-0 transition-transform ${props.open ? 'rotate-90' : ''}`} />
      <span class="flex-1 text-left truncate">{props.label}</span>
      <span
        class="tabular-nums"
        classList={{ 'text-attention': props.urgent === true }}
      >
        {props.count}
      </span>
    </button>
    <Show when={props.open}>{props.children}</Show>
  </Show>
);
