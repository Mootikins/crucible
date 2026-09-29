/**
 * The swap button: it chooses what opens in the centre, documents or
 * sessions, and moves nothing that is already open.
 */
import type { Component } from 'solid-js';
import { ArrowLeftRight } from 'lucide-solid';
import { RailButton } from './RailButton';

export type SpawnKind = 'docs' | 'sessions';

export const SpawnButton: Component<{ spawn: SpawnKind; onToggle: () => void }> = (props) => (
  <RailButton
    testId="mk-spawn"
    pressed={props.spawn === 'sessions'}
    title={
      props.spawn === 'docs'
        ? 'Documents open in the centre. Switch: sessions open there'
        : 'Sessions open in the centre. Switch: documents open there'
    }
    onClick={() => props.onToggle()}
  >
    <ArrowLeftRight class="w-4 h-4" />
  </RailButton>
);
