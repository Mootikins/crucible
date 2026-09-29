/** The inbox button, with the count of what waits for the user. */
import { Show, type Component } from 'solid-js';
import { Bell } from 'lucide-solid';
import { Pill } from '../primitives/Pill';
import { RailButton } from './RailButton';

export const InboxButton: Component<{ count: number; onClick: (e: MouseEvent) => void }> = (props) => (
  <RailButton title="Inbox" label={`Inbox, ${props.count} waiting`} onClick={props.onClick}>
    <Bell class="w-4 h-4" />
    <Show when={props.count}>
      <Pill kind="badge">{props.count}</Pill>
    </Show>
  </RailButton>
);
