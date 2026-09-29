/** A row of text choices, one of which is on: a radio group that looks like one control. */
import { For } from 'solid-js';

export interface SegmentedProps<V extends string> {
  value: V;
  /** Each option is `[value, label]`. */
  options: readonly (readonly [V, string])[];
  onChange: (v: V) => void;
}

export function Segmented<V extends string>(props: SegmentedProps<V>) {
  return (
    <div class="mk-tb-seg" role="radiogroup">
      <For each={props.options}>
        {([id, label]) => (
          <button
            type="button"
            role="radio"
            aria-checked={props.value === id}
            classList={{ on: props.value === id }}
            onClick={() => props.onChange(id)}
          >
            {label}
          </button>
        )}
      </For>
    </div>
  );
}
