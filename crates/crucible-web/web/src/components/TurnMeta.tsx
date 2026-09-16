/**
 * A turn's meta row: its actions and the measurements beside them, on one
 * line under the text. Hidden by opacity alone, so nothing above it moves
 * when it appears; hidden rows also take no pointer events. `always` keeps
 * the row on, for the turn that ends the transcript.
 */
import { Component, JSX } from 'solid-js';

export const TurnMeta: Component<{ always?: boolean; class?: string; children?: JSX.Element }> = (props) => (
  <div
    class={`flex items-center gap-2 text-floor leading-none text-muted-dark tabular-nums transition-opacity duration-200 ${
      props.always
        ? 'opacity-100'
        : 'opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:pointer-events-auto [@media(hover:none)]:opacity-100 [@media(hover:none)]:pointer-events-auto'
    } ${props.class ?? ''}`}
    data-testid="turn-meta"
  >
    {props.children}
  </div>
);

/** Who wrote the turn, for a screen reader only; never part of a copied selection. */
export const AuthorHeading: Component<{ children: string }> = (props) => (
  <h3 class="sr-only select-none">{props.children}</h3>
);
