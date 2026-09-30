/** The sessions, grouped by project, each group folds; a last row opens the full list. */
import { For, Show, type Component } from 'solid-js';
import { SessionGroupHeader } from './SessionGroupHeader';
import { SessionRow } from './SessionRow';
import type { SessionGroupView } from './types';

export interface SessionListProps {
  groups: SessionGroupView[];
  activeId: string;
  onOpen: (id: string) => void;
  /** Fold or unfold the group with this label. */
  onToggleGroup: (label: string) => void;
}

export const SessionList: Component<SessionListProps> = (props) => (
  <div class="mk-scroll mk-list">
    <For each={props.groups}>
      {(group) => (
        <>
          <SessionGroupHeader label={group.label} open={group.open} onToggle={() => props.onToggleGroup(group.label)} />
          <Show when={group.open}>
            <For each={group.sessions}>
              {(s) => <SessionRow session={s} current={props.activeId === s.id} onOpen={props.onOpen} />}
            </For>
          </Show>
        </>
      )}
    </For>
    <button type="button" class="mk-srow mk-more">
      <span />
      <span class="mk-t">All sessions</span>
      <span />
    </button>
  </div>
);
