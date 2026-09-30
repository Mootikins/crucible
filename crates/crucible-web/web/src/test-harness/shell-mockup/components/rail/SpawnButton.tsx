/**
 * The swap button: it chooses what opens in the centre, documents or
 * sessions, and moves nothing that is already open. A press shows nothing
 * else, so the icon takes the accent while sessions come first, and the
 * tooltip names the current choice.
 */
import type { Component } from 'solid-js';
import { ArrowLeftRight } from 'lucide-solid';
import { RailButton } from './RailButton';

export type SpawnKind = 'docs' | 'sessions';

/** The current choice, in words: for the tooltip and for the notice after a press. */
export const spawnText = (spawn: SpawnKind) =>
  spawn === 'sessions' ? 'New sessions open in the centre' : 'New documents open in the centre';

export const SpawnButton: Component<{ spawn: SpawnKind; onToggle: (e: MouseEvent) => void }> = (props) => (
  <RailButton
    testId="mk-spawn"
    pressed={props.spawn === 'sessions'}
    title={`${spawnText(props.spawn)}. Press to open new ${props.spawn === 'docs' ? 'sessions' : 'documents'} there`}
    onClick={(e) => props.onToggle(e)}
  >
    <ArrowLeftRight class="w-4 h-4" />
  </RailButton>
);
