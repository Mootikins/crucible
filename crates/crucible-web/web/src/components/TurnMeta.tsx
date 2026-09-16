/**
 * A turn's meta row — the actions it offers, and the measurements beside
 * them — copied from T3 Code's transcript.
 *
 * T3 gives a user message and an assistant turn the SAME arrangement: the row
 * sits at the bottom of the turn, in the flow, one small gap under the text,
 * aligned with the content's own edge rather than pushed to an edge of the
 * pane. It is hidden by opacity alone, and it fades in when the pointer
 * enters the turn. Two earlier attempts here failed in opposite directions:
 * an absolutely positioned strip under every turn, which cost each row a band
 * of reserved pixels, and a fixed right-hand column, which took 84 px out of
 * the reading measure and put the actions far from the words they act on.
 * This row costs no horizontal space, and no vertical space beyond the one
 * line it occupies.
 *
 * Only OPACITY answers the hover. A utility that changes a margin, a display
 * or a position moves an edge under the pointer, and the text above it is
 * what the reader aimed at. A device that reports no hover shows the row
 * always, because no gesture there can reveal it. While the row is hidden it
 * also takes no pointer events: a device that reports hover but taps (a
 * laptop with a touch screen) would otherwise land a tap on an invisible
 * button.
 *
 * `always` is T3's `alwaysVisible`: the turn that ENDS the transcript keeps
 * its meta row on, because there the row is the answer's footer rather than a
 * control the reader must go looking for.
 */
import { Component, JSX } from 'solid-js';

export const TurnMeta: Component<{ always?: boolean; children?: JSX.Element }> = (props) => (
  <div
    class={`flex items-center gap-2 text-floor leading-none text-muted-dark tabular-nums transition-opacity duration-200 ${
      props.always
        ? 'opacity-100'
        : 'opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:pointer-events-auto [@media(hover:none)]:opacity-100 [@media(hover:none)]:pointer-events-auto'
    }`}
    data-testid="turn-meta"
  >
    {props.children}
  </div>
);

/**
 * Who wrote the turn, for a screen reader only.
 *
 * A reader skims a transcript by heading, and ours carried no headings: a
 * screen reader met one unbroken run of text with no way to tell a prompt
 * from an answer. T3 answers this with a visually hidden heading on every
 * message, which costs a sighted reader nothing. `select-none` keeps the word
 * out of a copied selection.
 */
export const AuthorHeading: Component<{ children: string }> = (props) => (
  <h3 class="sr-only select-none">{props.children}</h3>
);
