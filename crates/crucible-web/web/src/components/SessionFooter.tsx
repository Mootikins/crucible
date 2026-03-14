import { Component, Show } from 'solid-js';
import type { Session } from '@/lib/types';
import { RefreshCw } from '@/lib/icons';
import { StateIndicator } from './SessionSection';

export const SessionFooter: Component<{
  session: Session;
  onPause: () => void;
  onResume: () => void;
  onRefresh: () => void;
}> = (props) => {
  return (
    <div class="border-t border-neutral-800 p-3">
      <div class="flex items-center gap-2 mb-2">
        <StateIndicator state={props.session.state} />
        <span class="text-sm font-medium">{props.session.state}</span>
      </div>

      <div class="flex gap-2">
        <Show when={props.session.state === 'active'}>
          <button
            onClick={props.onPause}
            class="flex-1 px-2 py-1 text-sm bg-yellow-600 text-white rounded hover:bg-yellow-700"
          >
            Pause
          </button>
        </Show>

        <Show when={props.session.state === 'paused'}>
          <button
            onClick={props.onResume}
            class="flex-1 px-2 py-1 text-sm bg-green-600 text-white rounded hover:bg-green-700"
          >
            Resume
          </button>
        </Show>


        <button
          onClick={props.onRefresh}
          class="px-2 py-1 text-sm bg-neutral-700 text-neutral-300 rounded hover:bg-neutral-600 flex items-center justify-center"
        >
          <RefreshCw class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
};
