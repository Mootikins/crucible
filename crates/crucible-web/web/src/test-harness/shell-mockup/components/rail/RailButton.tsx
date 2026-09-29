/**
 * An icon button in the rail's ribbon, for a control that the windowing
 * core's `RibbonCommand` cannot carry: a badge, a pressed state, or the
 * click event that anchors a popover.
 */
import type { Component, JSX } from 'solid-js';

export interface RailButtonProps {
  title: string;
  /** A fuller accessible name than the title, for example with a count. */
  label?: string;
  pressed?: boolean;
  testId?: string;
  onClick: (e: MouseEvent) => void;
  children: JSX.Element;
}

export const RailButton: Component<RailButtonProps> = (props) => (
  <button
    type="button"
    class="mk-railbtn"
    data-testid={props.testId}
    aria-label={props.label}
    aria-pressed={props.pressed}
    title={props.title}
    onClick={(e) => props.onClick(e)}
  >
    {props.children}
  </button>
);
