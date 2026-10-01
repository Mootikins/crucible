/**
 * A row of icon buttons, one of which is pressed (`.icon-toggle-group`). Unlike
 * `Segmented`, it shows icons only, so each option needs a title.
 */
import { For, type Component } from 'solid-js';
import { Dynamic } from 'solid-js/web';

export interface ToggleOption<V extends string> {
  value: V;
  title: string;
  icon: Component<{ class?: string }>;
}

export interface ToggleGroupProps<V extends string> {
  label: string;
  value: V;
  options: readonly ToggleOption<V>[];
  onChange: (v: V) => void;
}

export function ToggleGroup<V extends string>(props: ToggleGroupProps<V>) {
  return (
    <div class="icon-toggle-group" role="group" aria-label={props.label}>
      <For each={props.options}>
        {(o) => (
          <button type="button" aria-pressed={props.value === o.value} title={o.title} onClick={() => props.onChange(o.value)}>
            <Dynamic component={o.icon} />
          </button>
        )}
      </For>
    </div>
  );
}
