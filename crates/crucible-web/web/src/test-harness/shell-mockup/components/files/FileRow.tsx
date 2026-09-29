/**
 * A note row. It shows the count of hunks that wait for review
 * (`reviewStore.hunksForPath` in the real app); without one, a dot in the
 * session colour marks a note that the active session used.
 */
import { Show, type Component } from 'solid-js';
import { Pill } from '../primitives/Pill';

export interface FileRowProps {
  name: string;
  depth: number;
  current: boolean;
  pending: number;
  touched: boolean;
  color: string;
  onOpen: (e: MouseEvent) => void;
}

export const FileRow: Component<FileRowProps> = (props) => (
  <button
    type="button"
    class="mk-trow"
    style={{ 'padding-left': `${18 + props.depth * 14}px` }}
    aria-current={props.current ? 'page' : undefined}
    onClick={(e) => props.onOpen(e)}
  >
    <span class="mk-t">{props.name}</span>
    <Show
      when={props.pending}
      fallback={
        <Show when={props.touched}>
          <span class="mk-touch" style={{ background: props.color }} title="Used by this session" />
        </Show>
      }
    >
      <Pill kind="count" title={`${props.pending} to review`}>
        {props.pending}
      </Pill>
    </Show>
  </button>
);
