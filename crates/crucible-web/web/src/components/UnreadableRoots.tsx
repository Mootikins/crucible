/**
 * The roots that a diffset leaves out. The daemon cannot read them, so the
 * file list is not complete. Each row names the root and the reason.
 *
 * The diff pane and the Changes panel both show it, so that neither surface
 * lets the user read an incomplete record as complete.
 */
import { Component, For, Show } from 'solid-js';
import type { UnreadableRoot } from '@/lib/diffset';
import { AlertTriangle } from '@/lib/icons';

export const UnreadableRoots: Component<{ roots: UnreadableRoot[] }> = (props) => (
  <Show when={props.roots.length > 0}>
    <div
      class="mx-3 mt-2 rounded-md border border-attention/50 bg-attention/[0.06] px-3 py-1.5 text-xs"
      data-testid="diff-unreadable-roots"
      role="status"
    >
      <p class="flex items-center gap-2 text-shell-ink">
        <AlertTriangle class="h-3.5 w-3.5 shrink-0 text-attention" />
        This diff leaves out the files of these roots:
      </p>
      <ul class="mt-1 space-y-0.5 pl-5">
        <For each={props.roots}>
          {(unreadable) => (
            <li data-testid="diff-unreadable-root">
              <span class="font-mono text-shell-ink">{unreadable.root}</span>
              <span class="text-muted-dark"> — {unreadable.reason}</span>
            </li>
          )}
        </For>
      </ul>
    </div>
  </Show>
);
