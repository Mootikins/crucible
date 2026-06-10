import { Component, For } from 'solid-js';
import type { AutocompleteItem } from '@/hooks/useAutocomplete';

interface AutocompletePopupProps {
  items: AutocompleteItem[];
  selectedIndex: number;
  onSelect: (index: number) => void;
}

export const AutocompletePopup: Component<AutocompletePopupProps> = (props) => {
  return (
    <div class="absolute left-0 right-0 top-full mt-1 z-50 max-h-52 overflow-y-auto rounded-lg border border-neutral-700 bg-surface-elevated shadow-xl">
      <For each={props.items}>
        {(item, index) => (
          <button
            type="button"
            class="w-full px-3 py-2 text-left text-sm text-neutral-200 hover:bg-surface-overlay transition-colors"
            classList={{ 'bg-surface-overlay': index() === props.selectedIndex }}
            onMouseDown={(e) => {
              e.preventDefault();
              props.onSelect(index());
            }}
          >
            {item.label}
          </button>
        )}
      </For>
    </div>
  );
};
