/** One file in the review: its name, which opens the note, then its hunks. */
import type { Component, JSX } from 'solid-js';

export const ChangeFile: Component<{ path: string; onOpen: () => void; children: JSX.Element }> = (props) => (
  <section class="mk-cfile">
    <button type="button" class="mk-fname" onClick={() => props.onOpen()}>
      {props.path}.md
    </button>
    {props.children}
  </section>
);
