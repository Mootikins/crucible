/**
 * A message that waits for the running turn to end (`Message.queued` in the
 * real app), with the override that sends it now.
 */
import type { Component } from 'solid-js';
import { ArrowUp, X } from '@/lib/icons';
import { Button } from '../primitives/Button';
import { IconButton } from '../primitives/IconButton';

export const QueuedMessage: Component<{ text: string; onSendNow: () => void; onRemove: () => void }> = (props) => (
  <div class="mk-queued">
    <span class="mk-qlabel">Queued</span>
    <span class="mk-qtext">{props.text}</span>
    <Button title="Send now, without waiting for the turn to end" onClick={() => props.onSendNow()}>
      <ArrowUp class="mk-i" />
      Send now
    </Button>
    <IconButton label="Remove" labelAs="aria" onClick={() => props.onRemove()}>
      <X class="mk-i" />
    </IconButton>
  </div>
);
