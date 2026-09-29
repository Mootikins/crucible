/**
 * One colour choice, as a round button. `auto` draws the half-and-half mark
 * of "the theme decides"; it then needs no colour.
 */
import type { Component } from 'solid-js';

export interface SwatchProps {
  title: string;
  color?: string;
  on: boolean;
  auto?: boolean;
  onPick: () => void;
}

export const Swatch: Component<SwatchProps> = (props) => (
  <button
    type="button"
    title={props.title}
    aria-pressed={props.on}
    classList={{ 'mk-swatch': true, on: props.on, auto: !!props.auto }}
    style={{ background: props.color }}
    onClick={() => props.onPick()}
  />
);
