/**
 * The transcript's right-hand gutter — ONE column, on every row.
 *
 * Every turn, user and assistant alike, ends in a fixed column that holds
 * that turn's actions and, for an assistant turn, the elapsed time beside
 * them. The actions used to hang UNDER the turn on an absolute strip, which
 * cost every row a reserved band of empty pixels (`pb-5`, `mb-6`) whether or
 * not a pointer was ever near it. A column costs no vertical space at all.
 *
 * The reveal rule is the one the sessions rail already uses: the hidden state
 * keeps its box and changes only opacity, so nothing reflows when the pointer
 * arrives. A device with no hover shows the column always, because there is
 * no gesture there that could reveal it.
 */
import { Component, JSX } from 'solid-js';

export const TurnGutter: Component<{ children?: JSX.Element }> = (props) => (
  <div
    // `items-center` inside a row that is `items-start`: the column sits at
    // the TOP of the turn, and the elapsed time sits on the same line as the
    // button glyphs rather than on their top edge.
    //
    // Pointer events follow the opacity. An invisible button that still takes
    // a click is a trap, and the row under it is what the reader aimed at.
    class="pointer-events-none flex w-[var(--cru-turn-gutter)] shrink-0 items-center justify-end gap-0.5 opacity-0 transition-opacity duration-150 group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 [@media(hover:none)]:pointer-events-auto [@media(hover:none)]:opacity-100"
    data-testid="turn-gutter"
  >
    {props.children}
  </div>
);
