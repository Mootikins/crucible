/** The sessions, grouped by project, and a last row that opens the full list. */
import { For, type Component } from 'solid-js';
import { SectionLabel } from '../primitives/SectionLabel';
import { SessionRow } from './SessionRow';
import type { SessionGroupView } from './types';

export interface SessionListProps {
  groups: SessionGroupView[];
  activeId: string;
  onOpen: (id: string) => void;
}

export const SessionList: Component<SessionListProps> = (props) => (
  <div class="mk-scroll mk-list">
    <For each={props.groups}>
      {(group) => (
        <>
          <SectionLabel kind="group">{group.label}</SectionLabel>
          <For each={group.sessions}>
            {(s) => <SessionRow session={s} current={props.activeId === s.id} onOpen={props.onOpen} />}
          </For>
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
