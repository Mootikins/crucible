import { Component, For, createEffect } from 'solid-js';
import type { AutocompleteItem } from '@/hooks/useAutocomplete';

interface AutocompletePopupProps {
  items: AutocompleteItem[];
  selectedIndex: number;
  onSelect: (index: number) => void;
}

export const AutocompletePopup: Component<AutocompletePopupProps> = (props) => {
  const refs: (HTMLButtonElement | undefined)[] = [];

  // Keep the keyboard-selected row visible when the list overflows.
  createEffect(() => {
    refs[props.selectedIndex]?.scrollIntoView({ block: 'nearest' });
  });

  return (
    <div
      role="listbox"
      class="absolute left-0 right-0 top-full mt-1 z-50 max-h-52 overflow-y-auto rounded-lg border border-hairline bg-surface-elevated shadow-xl cru-anim-rise"
    >
      <For each={props.items}>
        {(item, index) => (
          <button
            ref={(el) => (refs[index()] = el)}
            type="button"
            role="option"
            aria-selected={index() === props.selectedIndex}
            // Keyboard selection reads as an ember tint (distinct from the
            // lighter hover wash) so it's clear what Enter will insert.
            class="w-full px-3 py-2 text-left text-sm text-shell-ink hover:bg-hover-wash transition-colors focus-visible:outline-none"
            classList={{ 'bg-primary/15': index() === props.selectedIndex }}
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
