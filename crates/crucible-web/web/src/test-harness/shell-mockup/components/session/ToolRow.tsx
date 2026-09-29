/**
 * The one quiet line of a tool call or a folded group: the icon and the
 * verb (a click opens the details), what the call touched, then the chevron.
 */
import type { Component, JSX } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { ChevronToggle } from '../primitives/ChevronToggle';

export interface ToolRowProps {
  icon: Component<{ class?: string }>;
  label: string;
  open: boolean;
  onToggle: () => void;
  /** What sits between the verb and the chevron: the target and the state. */
  children?: JSX.Element;
}

export const ToolRow: Component<ToolRowProps> = (props) => (
  <div class="mk-tlrow">
    <button type="button" class="mk-tltoggle" aria-expanded={props.open} onClick={() => props.onToggle()}>
      <Dynamic component={props.icon} class="mk-i" />
      <span class="mk-v">{props.label}</span>
    </button>
    {props.children}
    <ChevronToggle open={props.open} onToggle={props.onToggle} />
  </div>
);
