import { Component, For, Show, createSignal, createMemo } from 'solid-js';
import type { PopupRequest, PopupResponse } from '@/lib/types';

interface Props {
  request: PopupRequest;
  onRespond: (response: PopupResponse) => void;
}

export const PopupInteraction: Component<Props> = (props) => {
  const [filter, setFilter] = createSignal('');
  const [otherText, setOtherText] = createSignal('');

  const filteredEntries = createMemo(() => {
    const query = filter().toLowerCase();
    if (!query) return props.request.entries;
    return props.request.entries.filter(
      (entry) =>
        entry.label.toLowerCase().includes(query) ||
        entry.description?.toLowerCase().includes(query)
    );
  });

  const handleSelect = (index: number) => {
    const originalIndex = props.request.entries.findIndex(
      (e) => e === filteredEntries()[index]
    );
    props.onRespond({ selected_index: originalIndex });
  };

  const handleOther = () => {
    if (otherText().trim()) {
      props.onRespond({ other: otherText().trim() });
    }
  };

  return (
    <div class="bg-neutral-800 rounded-lg p-4 mb-4 border border-neutral-700">
      <p class="text-neutral-100 font-medium mb-3">{props.request.title}</p>

      <input
        type="text"
        placeholder="Search..."
        value={filter()}
        onInput={(e) => setFilter(e.currentTarget.value)}
        class="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-md text-neutral-100 placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-blue-500 mb-3"
      />

      <div class="max-h-48 overflow-y-auto space-y-1 mb-3">
        <For each={filteredEntries()}>
          {(entry, index) => (
            <button
              onClick={() => handleSelect(index())}
              class="w-full text-left px-3 py-2 rounded-md hover:bg-neutral-700 transition-colors"
            >
              <span class="text-neutral-100">{entry.label}</span>
              <Show when={entry.description}>
                <span class="text-neutral-500 text-sm block">{entry.description}</span>
              </Show>
            </button>
          )}
        </For>
        <Show when={filteredEntries().length === 0}>
          <p class="text-neutral-500 text-sm px-3 py-2">No matches found</p>
        </Show>
      </div>

      <Show when={props.request.allow_other}>
        <div class="flex gap-2">
          <input
            type="text"
            placeholder="Or type custom..."
            value={otherText()}
            onInput={(e) => setOtherText(e.currentTarget.value)}
            class="flex-1 px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-md text-neutral-100 placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
          <button
            onClick={handleOther}
            disabled={!otherText().trim()}
            class="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            Submit
          </button>
        </div>
      </Show>
    </div>
  );
};
