/**
 * The current app's review controls, as a quiet row under the header of the
 * changes view: the scope (the whole session, or the last turn) and a filter
 * that hides decided hunks. The toolbox variant "B with A's controls" shows it.
 */
import type { Component } from 'solid-js';
import { CheckRow } from '../primitives/CheckRow';
import { Segmented } from '../primitives/Segmented';

export type ReviewScope = 'session' | 'turn';

export interface ChangesControlsProps {
  scope: ReviewScope;
  onScope: (scope: ReviewScope) => void;
  unreviewedOnly: boolean;
  onUnreviewedOnly: (on: boolean) => void;
}

export const ChangesControls: Component<ChangesControlsProps> = (props) => (
  <div class="mk-review-controls">
    <Segmented
      value={props.scope}
      options={[['session', 'Session'], ['turn', 'Last turn']]}
      onChange={props.onScope}
    />
    <CheckRow checked={props.unreviewedOnly} onChange={props.onUnreviewedOnly}>
      Unreviewed only
    </CheckRow>
  </div>
);
