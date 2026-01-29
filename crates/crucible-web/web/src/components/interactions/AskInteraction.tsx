import { Component, For, Show, createSignal } from 'solid-js';
import type { AskRequest, AskResponse } from '@/lib/types';

interface Props {
  request: AskRequest;
  onRespond: (response: AskResponse) => void;
}

export const AskInteraction: Component<Props> = (props) => {
  const [selected, setSelected] = createSignal<number[]>([]);
  const [otherText, setOtherText] = createSignal('');

  const toggleChoice = (index: number) => {
    if (props.request.multi_select) {
      setSelected((prev) =>
        prev.includes(index) ? prev.filter((i) => i !== index) : [...prev, index]
      );
    } else {
      setSelected([index]);
    }
  };

  const handleSubmit = () => {
    const response: AskResponse = {
      selected: selected(),
      other: otherText().trim() || undefined,
    };
    props.onRespond(response);
  };

  const hasSelection = () => selected().length > 0 || otherText().trim().length > 0;

  return (
    <div class="bg-neutral-800 rounded-lg p-4 mb-4 border border-neutral-700">
      <p class="text-neutral-100 font-medium mb-3">{props.request.question}</p>

      <Show when={props.request.choices}>
        <div class="space-y-2 mb-3">
          <For each={props.request.choices}>
            {(choice, index) => (
              <label class="flex items-center gap-2 cursor-pointer group">
                <input
                  type={props.request.multi_select ? 'checkbox' : 'radio'}
                  name={`ask-${props.request.id}`}
                  checked={selected().includes(index())}
                  onChange={() => toggleChoice(index())}
                  class="w-4 h-4 text-blue-600 bg-neutral-700 border-neutral-600 focus:ring-blue-500"
                />
                <span class="text-neutral-300 group-hover:text-neutral-100">
                  {choice}
                </span>
              </label>
            )}
          </For>
        </div>
      </Show>

      <Show when={props.request.allow_other || !props.request.choices}>
        <input
          type="text"
          placeholder={props.request.choices ? 'Or type your own...' : 'Type your answer...'}
          value={otherText()}
          onInput={(e) => setOtherText(e.currentTarget.value)}
          class="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded-md text-neutral-100 placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-blue-500 mb-3"
        />
      </Show>

      <button
        onClick={handleSubmit}
        disabled={!hasSelection()}
        class="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      >
        Submit
      </button>
    </div>
  );
};
