/**
 * The state at the end of a tool line: it waits for permission, it failed,
 * its edit waits for review, or the user reverted it.
 */
import { Show, type Component } from 'solid-js';
import { StatusMark } from '../primitives/StatusMark';
import type { HunkState } from '../review/types';
import type { ToolState } from './types';

export const ToolStatus: Component<{ st: ToolState; out?: string; hunkState?: HunkState }> = (props) => (
  <>
    <Show when={props.st === 'ask'}>
      <span class="mk-tlst attn">
        <StatusMark status="need" />
        Waiting
      </span>
    </Show>
    <Show when={props.st === 'err'}>
      <span class="mk-tlst err">{props.out}</span>
    </Show>
    <Show when={props.st === 'review' && props.hunkState === 'pending'}>
      <span class="mk-tlst attn" title="Waits for your review">
        <StatusMark status="owe" />
      </span>
    </Show>
    <Show when={props.st === 'review' && props.hunkState === 'rejected'}>
      <span class="mk-tlst">Reverted</span>
    </Show>
  </>
);
