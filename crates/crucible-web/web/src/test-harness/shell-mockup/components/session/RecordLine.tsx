/** A quiet line of record in a session: a system event, not a message. */
import type { Component, JSX } from 'solid-js';

export const RecordLine: Component<{ children: JSX.Element }> = (props) => <div class="mk-record">{props.children}</div>;
