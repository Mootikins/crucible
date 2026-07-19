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
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <p class="text-shell-ink font-medium mb-3">{props.request.title}</p>

      <input
        type="text"
        placeholder="Search..."
        value={filter()}
        onInput={(e) => setFilter(e.currentTarget.value)}
        class="w-full px-3 py-2 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary mb-3"
      />

      <div class="max-h-48 overflow-y-auto space-y-1 mb-3">
        <For each={filteredEntries()}>
          {(entry, index) => (
            <button
              onClick={() => handleSelect(index())}
              class="w-full text-left px-3 py-2 rounded-md hover:bg-hover-wash transition-colors"
            >
              <span class="text-shell-ink">{entry.label}</span>
              <Show when={entry.description}>
                <span class="text-muted-dark text-sm block">{entry.description}</span>
              </Show>
            </button>
          )}
        </For>
        <Show when={filteredEntries().length === 0}>
          <p class="text-muted-dark text-sm px-3 py-2">No matches found</p>
        </Show>
      </div>

      <Show when={props.request.allow_other}>
        <div class="flex gap-2">
          <input
            type="text"
            placeholder="Or type custom..."
            value={otherText()}
            onInput={(e) => setOtherText(e.currentTarget.value)}
            class="flex-1 px-3 py-2 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary"
          />
          <button
            onClick={handleOther}
            disabled={!otherText().trim()}
            class="px-4 py-2 bg-primary text-white rounded-md hover:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            Submit
          </button>
        </div>
      </Show>
    </div>
  );
};
