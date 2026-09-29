/** The context meter: a ring that fills with the part of the context window in use. */
import type { Component } from 'solid-js';

const CIRCUMFERENCE = 2 * Math.PI * 6;

export const ContextRing: Component<{ pct: number }> = (props) => (
  <span class="mk-ctxring" title={`${props.pct}% of the context window used`}>
    <svg class="mk-ring" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="6" />
      <circle cx="8" cy="8" r="6" class="v" style={{ 'stroke-dasharray': `${(props.pct / 100) * CIRCUMFERENCE} ${CIRCUMFERENCE}` }} />
    </svg>
  </span>
);
