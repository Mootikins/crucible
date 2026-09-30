/**
 * The header of one project group: a caret in the column of the row marks,
 * then the project name. A click folds or unfolds the group, as the project
 * rows of the current app's `SessionTree` do.
 */
import type { Component } from 'solid-js';
import { Caret } from '../primitives/Caret';

export const SessionGroupHeader: Component<{ label: string; open: boolean; onToggle: () => void }> = (props) => (
  <button type="button" class="mk-grouplabel mk-grouphead" aria-expanded={props.open} onClick={() => props.onToggle()}>
    <Caret open={props.open} />
    <span class="mk-t">{props.label}</span>
  </button>
);
