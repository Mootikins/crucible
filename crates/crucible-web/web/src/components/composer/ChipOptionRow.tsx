import { type Component, Show } from 'solid-js';
import { ChevronRight, Check } from '@/lib/icons';
import type { ChipOption } from './ChipSelect';

/** Shared choice row for the picker list and its detached submenu. */
export const ChipOptionRow: Component<{
  option: ChipOption;
  selected: boolean;
  hovered: boolean;
  testid?: string;
  submenu?: boolean;
  expanded?: boolean;
  onHover: (event: MouseEvent & { currentTarget: HTMLButtonElement }) => void;
  onActivate: (event: MouseEvent & { currentTarget: HTMLButtonElement }) => void;
}> = (props) => (
    <button
      type="button"
      role="option"
      aria-selected={props.selected}
      disabled={props.option.disabled}
      onMouseEnter={props.onHover}
      onClick={props.onActivate}
      aria-haspopup={props.submenu ? 'menu' : undefined}
      aria-expanded={props.submenu ? props.expanded : undefined}
      data-testid={props.testid}
      classList={{
        'rounded-control w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors': true,
        'bg-hover-wash': props.hovered,
        'text-shell-ink': !props.option.disabled,
        'text-muted-dark cursor-not-allowed': !!props.option.disabled,
      }}
    >
      <Show when={props.option.icon} keyed>
        {(Icon) => <Icon class="w-3.5 h-3.5 flex-shrink-0 text-muted-dark" />}
      </Show>
      <span class="truncate">{props.option.label}</span>
      <Show when={props.selected}>
        <Check class="w-3.5 h-3.5 flex-shrink-0 text-primary" />
      </Show>
      <Show when={props.option.hint}>
        <span class="ml-auto pl-3 text-muted-dark truncate max-w-[140px]">
          {props.option.hint}
        </span>
      </Show>
      <Show when={props.submenu}>
        <ChevronRight
          class={`w-3.5 h-3.5 flex-shrink-0 text-muted-dark ${props.option.hint ? '' : 'ml-auto'}`}
        />
      </Show>
    </button>
);
