/** The note that goes with the next message, as a chip; its button takes it off. */
import type { Component } from 'solid-js';
import { Link, X } from 'lucide-solid';

export const ContextChip: Component<{ name: string; onRemove: () => void }> = (props) => (
  <span class="mk-chip" title="The open note goes with your message">
    <Link class="mk-i" />
    <span class="mk-t">{props.name}</span>
    <button type="button" aria-label="Do not send this note" onClick={() => props.onRemove()}>
      <X class="mk-i" />
    </button>
  </span>
);
