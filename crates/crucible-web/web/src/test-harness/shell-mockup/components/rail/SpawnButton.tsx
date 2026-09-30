/**
 * The swap button: the centre and the right rail trade places, so sessions
 * or documents take the centre. New tabs of each kind follow them. The icon
 * takes the accent while sessions hold the centre.
 */
import type { Component } from 'solid-js';
import { ArrowLeftRight } from 'lucide-solid';
import { RailButton } from './RailButton';

export type SpawnKind = 'docs' | 'sessions';

/** The current choice, in words: for the tooltip and for the notice after a press. */
export const spawnText = (spawn: SpawnKind) =>
  spawn === 'sessions' ? 'Sessions in the centre' : 'Documents in the centre';

export const SpawnButton: Component<{ spawn: SpawnKind; onToggle: (e: MouseEvent) => void }> = (props) => (
  <RailButton
    testId="mk-spawn"
    pressed={props.spawn === 'sessions'}
    title={`${spawnText(props.spawn)}. Press to swap the centre and the right rail`}
    onClick={(e) => props.onToggle(e)}
  >
    <ArrowLeftRight class="w-4 h-4" />
  </RailButton>
);
