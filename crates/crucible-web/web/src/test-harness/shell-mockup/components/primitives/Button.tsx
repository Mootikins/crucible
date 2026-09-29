/** The small text button (`.mk-btn sm`). Every text button of the mockup is this size. */
import type { Component, JSX } from 'solid-js';

export interface ButtonProps {
  /** No variant: the raised default. */
  variant?: 'primary' | 'ghost';
  /** The pointer turns the label to the error colour. */
  danger?: boolean;
  title?: string;
  disabled?: boolean;
  onClick?: (e: MouseEvent) => void;
  children: JSX.Element;
}

export const Button: Component<ButtonProps> = (props) => (
  <button
    type="button"
    class={`mk-btn sm${props.variant ? ` ${props.variant}` : ''}${props.danger ? ' danger' : ''}`}
    title={props.title}
    disabled={props.disabled}
    onClick={(e) => props.onClick?.(e)}
  >
    {props.children}
  </button>
);
