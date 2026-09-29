/**
 * A quiet text button for a session knob (the permission mode, the model).
 * The real app fills it from `useSessionModes` and `useSessionModels`.
 */
import type { Component, JSX } from 'solid-js';

export const KnobButton: Component<{ title: string; children: JSX.Element }> = (props) => (
  <button type="button" class="mk-quietbtn" title={props.title}>
    {props.children}
  </button>
);
