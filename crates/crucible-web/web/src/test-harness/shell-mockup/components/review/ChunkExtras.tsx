/**
 * The current app's per-hunk tools: undo a decision, and comment on the hunk.
 * They sit beside the decision buttons of a chunk.
 */
import { Show, type Component } from 'solid-js';
import { MessageSquare, Undo2 } from 'lucide-solid';
import { IconButton } from '../primitives/IconButton';

export interface ChunkExtrasProps {
  /** A decided hunk can go back to unreviewed. */
  decided: boolean;
  onUndo: () => void;
  onComment: () => void;
}

export const ChunkExtras: Component<ChunkExtrasProps> = (props) => (
  <>
    <Show when={props.decided}>
      <IconButton label="Undo the decision" onClick={() => props.onUndo()}>
        <Undo2 class="mk-i" />
      </IconButton>
    </Show>
    <IconButton label="Comment on this change" onClick={() => props.onComment()}>
      <MessageSquare class="mk-i" />
    </IconButton>
  </>
);
