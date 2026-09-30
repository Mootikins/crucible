/**
 * A file row. It shows the count of hunks that wait for review
 * (`the pending proposals for this path` in the real app); without one, a dot in the
 * session colour marks a note that the active session used. The file
 * labels setting adds an icon to each file, or an extension label to each
 * file that is not a note.
 */
import { Show, type Component } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { Pill } from '../primitives/Pill';
import { fileIcon, fileLabel, type FileLabels } from './fileLabel';

export interface FileRowProps {
  /** The file name, with its extension. */
  name: string;
  depth: number;
  labels: FileLabels;
  current: boolean;
  pending: number;
  touched: boolean;
  color: string;
  onOpen: (e: MouseEvent) => void;
}

export const FileRow: Component<FileRowProps> = (props) => {
  const label = () => fileLabel(props.name);
  return (
    <button
      type="button"
      class="mk-trow"
      style={{ '--mk-depth': props.depth }}
      aria-current={props.current ? 'page' : undefined}
      onClick={(e) => props.onOpen(e)}
    >
      <Show when={props.labels === 'icons'}>
        <Dynamic component={fileIcon(label())} class="mk-i" />
      </Show>
      <span class="mk-t">{label().title}</span>
      <Show when={props.labels === 'extensions' && label().ext}>
        <span class="mk-ext">{label().ext}</span>
      </Show>
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
};
