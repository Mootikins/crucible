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
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <p class="text-shell-ink font-medium mb-3">{props.request.question}</p>

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
                  class="w-4 h-4 text-primary bg-control border-hairline focus:ring-primary"
                />
                <span class="text-shell-body group-hover:text-shell-ink">
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
          class="w-full px-3 py-2 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary mb-3"
        />
      </Show>

      <button
        onClick={handleSubmit}
        disabled={!hasSelection()}
        class="px-4 py-2 bg-primary text-white rounded-md hover:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      >
        Submit
      </button>
    </div>
  );
};
