/** A folder row. Each level of depth indents it by 14px. */
import type { Component } from 'solid-js';
import { Caret } from '../primitives/Caret';

export const DirRow: Component<{ name: string; depth: number; open: boolean; onToggle: () => void }> = (props) => (
  <button type="button" class="mk-trow mk-dir" style={{ 'padding-left': `${4 + props.depth * 14}px` }} onClick={() => props.onToggle()}>
    <Caret open={props.open} />
    <span class="mk-t">{props.name}</span>
  </button>
);
