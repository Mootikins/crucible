import { Component, Show } from 'solid-js';
import type { InteractionRequest, InteractionResponse } from '@/lib/types';
import { InteractionHandler } from './interactions';

interface Props {
  request: InteractionRequest | null;
  onRespond: (response: InteractionResponse) => void;
}

/**
 * Modal overlay for interaction requests.
 * Wraps InteractionHandler in a backdrop + centered card for prominent display.
 * Used when interactions need to be surfaced outside inline chat flow.
 */
export const InteractionModal: Component<Props> = (props) => {
  return (
    <Show when={props.request}>
      {(request) => (
        <div class="fixed inset-0 z-[100] flex items-center justify-center">
          {/* Backdrop */}
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" />

          {/* Modal card */}
          <div class="relative z-10 w-full max-w-2xl mx-4 max-h-[80vh] overflow-y-auto">
            <InteractionHandler request={request()} onRespond={props.onRespond} />
          </div>
        </div>
      )}
    </Show>
  );
};
