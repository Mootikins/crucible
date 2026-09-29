/**
 * An icon-only button. It has no fill: the pointer and the pressed state
 * change the icon colour (`.mk-iconbtn` in mockup.css).
 */
import type { Component, JSX } from 'solid-js';

export interface IconButtonProps {
  /** The accessible name. A button that shows only an icon has no other name. */
  label: string;
  /** `title` also shows the name as a tooltip. `aria` gives only the name. */
  labelAs?: 'title' | 'aria';
  pressed?: boolean;
  disabled?: boolean;
  /** More classes after `mk-iconbtn`. */
  class?: string;
  onClick?: (e: MouseEvent) => void;
  children: JSX.Element;
}

export const IconButton: Component<IconButtonProps> = (props) => (
  <button
    type="button"
    class={props.class ? `mk-iconbtn ${props.class}` : 'mk-iconbtn'}
    title={props.labelAs === 'aria' ? undefined : props.label}
    aria-label={props.labelAs === 'aria' ? props.label : undefined}
    aria-pressed={props.pressed}
    disabled={props.disabled}
    onClick={(e) => props.onClick?.(e)}
  >
    {props.children}
  </button>
);
