import { Component, For, Show, createMemo, createSignal } from 'solid-js';
import type { InteractionOf, PanelItem, PanelResponse } from '@/lib/types';
import { btnPrimary, btnNeutral } from '@/lib/button-style';

interface Props {
  request: InteractionOf<'panel'>;
  onRespond: (response: PanelResponse) => void;
}

/**
 * The scripted-UI primitive: a list with optional filtering and multi-select.
 *
 * Distinct from `popup` on the wire and here. A popup answers with one index
 * or free text; a panel answers with a set, and its `hints` decide whether the
 * user can filter, pick several, or type something not on the list.
 *
 * Selection indices are always into the ORIGINAL items array, never into the
 * filtered view. The filter is a display concern and the asker never sees it,
 * so returning a filtered index would mean the caller resolving a selection
 * against a list it cannot reconstruct.
 */
export const PanelInteraction: Component<Props> = (props) => {
  const hints = () => props.request.hints ?? {};
  const [filter, setFilter] = createSignal(hints().initial_filter ?? '');
  const [selected, setSelected] = createSignal<number[]>(hints().initial_selection ?? []);
  const [other, setOther] = createSignal('');

  /** `[originalIndex, item]` pairs surviving the filter. */
  const visible = createMemo<[number, PanelItem][]>(() => {
    const pairs = props.request.items.map((item, i) => [i, item] as [number, PanelItem]);
    const query = filter().trim().toLowerCase();
    if (!query || !hints().filterable) return pairs;
    return pairs.filter(
      ([, item]) =>
        item.label.toLowerCase().includes(query) ||
        (item.description?.toLowerCase().includes(query) ?? false)
    );
  });

  const choose = (originalIndex: number) => {
    if (hints().multi_select) {
      setSelected((prev) =>
        prev.includes(originalIndex)
          ? prev.filter((i) => i !== originalIndex)
          : [...prev, originalIndex]
      );
      return;
    }
    // Single-select is a click-to-answer list, like the popup it resembles.
    props.onRespond({ kind: 'panel', selected: [originalIndex] });
  };

  const submit = () =>
    props.onRespond({
      kind: 'panel',
      selected: selected(),
      other: other().trim() || undefined,
    });

  const cancel = () => props.onRespond({ kind: 'panel', cancelled: true, selected: [] });

  const canSubmit = () => selected().length > 0 || other().trim().length > 0;

  return (
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <p class="text-shell-ink font-medium mb-3">{props.request.header}</p>

      <Show when={hints().filterable}>
        <input
          type="text"
          placeholder="Filter..."
          value={filter()}
          onInput={(e) => setFilter(e.currentTarget.value)}
          class="w-full px-3 py-2 mb-3 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary"
        />
      </Show>

      <div class="space-y-1 mb-3 max-h-72 overflow-y-auto">
        <For each={visible()}>
          {([originalIndex, item]) => (
            <button
              onClick={() => choose(originalIndex)}
              class={`w-full text-left px-3 py-2 rounded-md border transition-colors ${
                selected().includes(originalIndex)
                  ? 'border-primary bg-control'
                  : 'border-transparent hover:bg-control'
              }`}
            >
              <span class="text-shell-ink">{item.label}</span>
              <Show when={item.description}>
                <span class="block text-muted-dark text-xs">{item.description}</span>
              </Show>
            </button>
          )}
        </For>
        <Show when={visible().length === 0}>
          <p class="text-muted-dark text-sm px-3 py-2">No matching items</p>
        </Show>
      </div>

      <Show when={hints().allow_other}>
        <input
          type="text"
          placeholder="Or type your own..."
          value={other()}
          onInput={(e) => setOther(e.currentTarget.value)}
          class="w-full px-3 py-2 mb-3 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary"
        />
      </Show>

      <div class="flex items-center gap-2">
        <Show when={hints().multi_select || hints().allow_other}>
          <button onClick={submit} disabled={!canSubmit()} class={btnPrimary}>
            Confirm
          </button>
        </Show>
        <button onClick={cancel} class={btnNeutral}>
          Cancel
        </button>
      </div>
    </div>
  );
};
