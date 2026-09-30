/**
 * The head of a session: its identity and title, the context meter, the
 * review count, the expand toggle and the menu.
 */
import { Show, type Component } from 'solid-js';
import { GitCompare, Maximize2, Minimize2, MoreHorizontal } from 'lucide-solid';
import { ContextRing } from '../primitives/ContextRing';
import { IconButton } from '../primitives/IconButton';
import { Ident } from '../primitives/Ident';

export interface SessionHeaderProps {
  title: string;
  color: string;
  /** The part of the context window in use, in percent. */
  ctx: number;
  /** The hunks that wait for review. */
  pending: number;
  /** The session covers the centre. */
  expanded: boolean;
  onReview: () => void;
  /** Absent where the session cannot cover the centre: in the centre itself. */
  onToggleExpand?: () => void;
}

export const SessionHeader: Component<SessionHeaderProps> = (props) => (
  <div class="mk-shead">
    <Ident color={props.color} />
    <span class="mk-stitle">{props.title}</span>
    <ContextRing pct={props.ctx} />
    <Show when={props.pending}>
      <IconButton class="mk-reviewbtn" label={`${props.pending} to review`} onClick={() => props.onReview()}>
        <GitCompare class="mk-i" />
        {props.pending}
      </IconButton>
    </Show>
    <Show when={props.onToggleExpand}>
      {(toggle) => (
        <IconButton
          label={`${props.expanded ? 'Back to the documents' : 'Cover the centre'} (Shift+Esc)`}
          pressed={props.expanded}
          onClick={() => toggle()()}
        >
          <Show when={props.expanded} fallback={<Maximize2 class="mk-i" />}>
            <Minimize2 class="mk-i" />
          </Show>
        </IconButton>
      )}
    </Show>
    <IconButton label="More">
      <MoreHorizontal class="mk-i" />
    </IconButton>
  </div>
);
