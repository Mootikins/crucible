/** The head of the review: the session it belongs to, and the buttons that decide every open hunk. */
import type { Component } from 'solid-js';
import { DecisionButtons } from '../primitives/DecisionButtons';
import { Ident } from '../primitives/Ident';
import type { HunkAuthor } from './types';

export interface ChangesHeaderProps {
  session: HunkAuthor;
  /** The count of hunks that wait. With none, the buttons do nothing. */
  pending: number;
  onDecideAll: (accept: boolean) => void;
}

export const ChangesHeader: Component<ChangesHeaderProps> = (props) => (
  <header>
    <h2>
      Changes
      <small>
        <Ident color={props.session.color} />
        {props.session.title}
      </small>
    </h2>
    <DecisionButtons
      primary
      rejectLabel="Reject all"
      acceptLabel="Accept all"
      disabled={!props.pending}
      onReject={() => props.onDecideAll(false)}
      onAccept={() => props.onDecideAll(true)}
    />
  </header>
);
