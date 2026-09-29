/** The review of one session: every file it changed, hunk by hunk. */
import { For, type Component, type JSX } from 'solid-js';
import { EmptyText } from '../primitives/EmptyText';
import { ChangeChunk } from './ChangeChunk';
import { ChangeFile } from './ChangeFile';
import { ChangesHeader } from './ChangesHeader';
import type { ChangeFileView, HunkAuthor, HunkView } from './types';

export interface ChangesViewProps {
  session: HunkAuthor;
  files: ChangeFileView[];
  pending: number;
  onDecideAll: (accept: boolean) => void;
  onDecide: (hunkId: string, accept: boolean) => void;
  onOpenFile: (path: string) => void;
  /** A row under the header, for example the scope and the filter. */
  controls?: JSX.Element;
  /** More tools for each hunk. */
  hunkExtras?: (hunk: HunkView) => JSX.Element;
}

export const ChangesView: Component<ChangesViewProps> = (props) => (
  <div class="mk-scroll">
    <div class="mk-changes">
      <ChangesHeader session={props.session} pending={props.pending} onDecideAll={props.onDecideAll} />
      {props.controls}
      <For each={props.files} fallback={<EmptyText as="p">Nothing to review.</EmptyText>}>
        {(file) => (
          <ChangeFile path={file.path} onOpen={() => props.onOpenFile(file.path)}>
            <For each={file.hunks}>{(h) => <ChangeChunk hunk={h} onDecide={(accept) => props.onDecide(h.id, accept)} extras={props.hunkExtras?.(h)} />}</For>
          </ChangeFile>
        )}
      </For>
    </div>
  </div>
);
