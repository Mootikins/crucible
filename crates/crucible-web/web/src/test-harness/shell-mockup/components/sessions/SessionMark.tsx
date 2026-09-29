/** The mark at the start of a session row: the state mark, or the identity colour when idle. */
import { Show, type Component } from 'solid-js';
import { Ident } from '../primitives/Ident';
import { StatusMark } from '../primitives/StatusMark';
import type { SessionMarkStatus } from './types';

const TITLE = { need: 'Needs you', run: 'Working', owe: 'Changes to review' } as const;

export const SessionMark: Component<{ status: SessionMarkStatus; color: string }> = (props) => (
  <Show when={props.status !== 'idle' && props.status} fallback={<Ident color={props.color} dim />}>
    {(status) => <StatusMark status={status()} title={TITLE[status()]} />}
  </Show>
);
