/** A compact, scrolling diff of one change, in the mono face. */
import type { Component } from 'solid-js';
import { DiffLines, type DiffLinesProps } from './DiffLines';

export const MiniDiff: Component<DiffLinesProps> = (props) => (
  <pre class="mk-mini">
    <DiffLines del={props.del} add={props.add} />
  </pre>
);
