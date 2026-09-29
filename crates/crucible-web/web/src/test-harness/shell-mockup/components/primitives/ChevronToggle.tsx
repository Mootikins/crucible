/**
 * The quiet chevron at the end of a row. It turns a quarter when the row
 * opens; the stylesheet keeps it faint, never hidden, because touch has no hover.
 */
import type { Component } from 'solid-js';
import { ChevronRight } from 'lucide-solid';

export const ChevronToggle: Component<{ open: boolean; onToggle: () => void; label?: string }> = (props) => (
  <button type="button" class="mk-chev" aria-label={props.label ?? 'Details'} aria-expanded={props.open} onClick={() => props.onToggle()}>
    <ChevronRight class="mk-i" />
  </button>
);
