import { Component, For, Show, createSignal } from 'solid-js';
import type { AskBatchResponse, InteractionOf, QuestionAnswer } from '@/lib/types';
import { btnPrimary, btnNeutral } from '@/lib/button-style';

interface Props {
  request: InteractionOf<'ask_batch'>;
  onRespond: (response: AskBatchResponse) => void;
}

/**
 * One to four questions answered together.
 *
 * Not a loop over `AskInteraction`: the point of a batch is that the user sees
 * every question before answering any of them, so the answers are submitted as
 * one act. Rendering them as a queue of single asks would lose exactly the
 * property the variant exists for.
 */
export const AskBatchInteraction: Component<Props> = (props) => {
  const [selected, setSelected] = createSignal<number[][]>(
    props.request.questions.map(() => [])
  );
  const [other, setOther] = createSignal<string[]>(props.request.questions.map(() => ''));

  const toggle = (qi: number, ci: number) => {
    setSelected((prev) =>
      prev.map((picks, i) => {
        if (i !== qi) return picks;
        if (props.request.questions[qi]?.multi_select) {
          return picks.includes(ci) ? picks.filter((p) => p !== ci) : [...picks, ci];
        }
        return [ci];
      })
    );
  };

  const setOtherAt = (qi: number, text: string) =>
    setOther((prev) => prev.map((t, i) => (i === qi ? text : t)));

  const answered = (qi: number) =>
    (selected()[qi]?.length ?? 0) > 0 || (other()[qi]?.trim().length ?? 0) > 0;

  // Every question must have an answer: a partial batch would arrive as an
  // array with holes, and the asker has no way to tell a skipped question from
  // an empty selection.
  const complete = () => props.request.questions.every((_, i) => answered(i));

  const submit = () => {
    const answers: QuestionAnswer[] = props.request.questions.map((_, i) => ({
      selected: selected()[i] ?? [],
      other: other()[i]?.trim() || undefined,
    }));
    props.onRespond({ kind: 'ask_batch', id: props.request.id, answers });
  };

  const cancel = () =>
    props.onRespond({
      kind: 'ask_batch',
      id: props.request.id,
      answers: [],
      cancelled: true,
    });

  return (
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <For each={props.request.questions}>
        {(question, qi) => (
          <div class="mb-4 last:mb-3">
            <Show when={question.header}>
              <span class="inline-block px-2 py-0.5 mb-1 rounded bg-control text-muted-dark text-xs">
                {question.header}
              </span>
            </Show>
            <p class="text-shell-ink font-medium mb-2">{question.question}</p>

            <div class="space-y-2 mb-2">
              <For each={question.choices}>
                {(choice, ci) => (
                  <label class="flex items-center gap-2 cursor-pointer group">
                    <input
                      type={question.multi_select ? 'checkbox' : 'radio'}
                      name={`ask-batch-${props.request.id}-${qi()}`}
                      checked={selected()[qi()]?.includes(ci()) ?? false}
                      onChange={() => toggle(qi(), ci())}
                      class="w-4 h-4 text-primary bg-control border-hairline focus:ring-primary"
                    />
                    <span class="text-shell-body group-hover:text-shell-ink">{choice}</span>
                  </label>
                )}
              </For>
            </div>

            <Show when={question.allow_other || question.choices.length === 0}>
              <input
                type="text"
                placeholder={
                  question.choices.length > 0 ? 'Or type your own...' : 'Type your answer...'
                }
                value={other()[qi()] ?? ''}
                onInput={(e) => setOtherAt(qi(), e.currentTarget.value)}
                class="w-full px-3 py-2 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </Show>
          </div>
        )}
      </For>

      <div class="flex items-center gap-2">
        <button onClick={submit} disabled={!complete()} class={btnPrimary}>
          Submit
        </button>
        <button onClick={cancel} class={btnNeutral}>
          Cancel
        </button>
      </div>
    </div>
  );
};
