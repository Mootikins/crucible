/**
 * A session's identity colour, as a small square. The real `Session` has no
 * colour field; a port derives the colour from the session id, or drops it.
 */
import type { Component } from 'solid-js';

export const Ident: Component<{ color: string; dim?: boolean }> = (props) => (
  <span class="mk-ident" style={{ background: props.color, opacity: props.dim ? 0.55 : undefined }} />
);
