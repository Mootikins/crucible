/** A checkbox with a label and a quieter note under the label. */
import type { Component, JSX } from 'solid-js';

export interface CheckRowProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  children: JSX.Element;
  note?: string;
}

export const CheckRow: Component<CheckRowProps> = (props) => (
  <label class="mk-check">
    <input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
    <span>
      {props.children}
      <small>{props.note}</small>
    </span>
  </label>
);
