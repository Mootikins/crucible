import { Component, For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { ChevronDown, Check } from '@/lib/icons';

export interface ChipOption {
  value: string;
  label: string;
  /** Dimmed suffix (e.g. a path or availability note). */
  hint?: string;
  disabled?: boolean;
  /** Section header rendered above this option when it differs from the
   * previous option's group (caller keeps options sorted by group). */
  group?: string;
}

/**
 * Context chip with a custom popout list — the composer's picker idiom
 * (repo/branch-switcher style): a quiet inline `label ˅` trigger, a floating
 * panel with the options, a filter box once the list is big enough to need
 * one, and a check mark on the current selection. Native <select> can't do
 * any of that (no search, no hints, no styling of the list).
 */
export const ChipSelect: Component<{
  /** Leading static label, e.g. "kiln". Screen-reader name for the trigger. */
  name: string;
  options: ChipOption[];
  value: string;
  onSelect: (value: string) => void;
  /** Shown on the chip when value is '' and no option matches. */
  placeholder?: string;
  disabled?: boolean;
  testid?: string;
  /** Show the filter input at ≥ this many options (default 8). */
  searchThreshold?: number;
  /** Leading icon on the trigger chip. */
  icon?: Component<{ class?: string }>;
}> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [filter, setFilter] = createSignal('');
  const [hover, setHover] = createSignal(-1);
  let rootRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;

  const current = () => props.options.find((o) => o.value === props.value);
  const display = () => current()?.label ?? props.placeholder ?? props.name;
  const searchable = () => props.options.length >= (props.searchThreshold ?? 8);
  const visible = () => {
    const q = filter().trim().toLowerCase();
    if (!q) return props.options;
    return props.options.filter(
      (o) => o.label.toLowerCase().includes(q) || o.hint?.toLowerCase().includes(q),
    );
  };

  const close = () => {
    setOpen(false);
    setFilter('');
    setHover(-1);
  };

  const pick = (o: ChipOption) => {
    if (o.disabled) return;
    props.onSelect(o.value);
    close();
  };

  createEffect(() => {
    if (!open()) return;
    queueMicrotask(() => inputRef?.focus());
    const onDocClick = (e: MouseEvent) => {
      if (rootRef && !rootRef.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        close();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const list = visible();
        if (!list.length) return;
        const delta = e.key === 'ArrowDown' ? 1 : -1;
        setHover((h) => (h + delta + list.length) % list.length);
      } else if (e.key === 'Enter') {
        const o = visible()[hover()] ?? visible()[0];
        if (o) {
          e.preventDefault();
          pick(o);
        }
      }
    };
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
    onCleanup(() => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKey);
    });
  });

  return (
    <div ref={rootRef} class="relative inline-flex">
      <button
        type="button"
        aria-label={props.name}
        aria-expanded={open()}
        disabled={props.disabled}
        data-testid={props.testid}
        onClick={() => (open() ? close() : setOpen(true))}
        classList={{
          'inline-flex items-center gap-1 px-2 py-1 rounded-md text-xs transition-colors max-w-[220px]': true,
          'text-shell-body hover:bg-hover-wash': !open(),
          'bg-hover-wash text-shell-ink': open(),
          'opacity-50 cursor-not-allowed': props.disabled,
        }}
      >
        <Show when={props.icon} keyed>
          {(Icon) => <Icon class="w-3.5 h-3.5 flex-shrink-0 text-muted-dark" />}
        </Show>
        <span class="truncate">{display()}</span>
        <ChevronDown class="w-3 h-3 flex-shrink-0 text-muted-dark" />
      </button>

      <Show when={open()}>
        <div
          data-testid={props.testid ? `${props.testid}-popout` : undefined}
          class="absolute left-0 top-full mt-1 z-50 min-w-[220px] max-w-[320px] bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 cru-anim-rise"
        >
          <Show when={searchable()}>
            <div class="px-2 pb-1 pt-0.5 border-b border-hairline">
              <input
                ref={inputRef}
                value={filter()}
                onInput={(e) => {
                  setFilter(e.currentTarget.value);
                  setHover(0);
                }}
                placeholder={`Search ${props.name}…`}
                aria-label={`Search ${props.name}`}
                class="w-full bg-transparent text-xs text-shell-ink placeholder-muted-dark outline-none py-1"
              />
            </div>
          </Show>
          <div class="max-h-[300px] overflow-y-auto" role="listbox" aria-label={props.name}>
            <For each={visible()}>
              {(o, i) => (
                <>
                <Show when={o.group && o.group !== visible()[i() - 1]?.group}>
                  <div class="px-3 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-dark">
                    {o.group}
                  </div>
                </Show>
                <button
                  type="button"
                  role="option"
                  aria-selected={o.value === props.value}
                  disabled={o.disabled}
                  onMouseEnter={() => setHover(i())}
                  onClick={() => pick(o)}
                  classList={{
                    'w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors': true,
                    'bg-hover-wash': hover() === i(),
                    'text-shell-ink': !o.disabled,
                    'text-muted-dark cursor-not-allowed': !!o.disabled,
                  }}
                >
                  <span class="w-3.5 flex-shrink-0">
                    <Show when={o.value === props.value}>
                      <Check class="w-3.5 h-3.5 text-primary" />
                    </Show>
                  </span>
                  <span class="truncate">{o.label}</span>
                  <Show when={o.hint}>
                    <span class="ml-auto pl-3 text-muted-dark truncate max-w-[140px]">{o.hint}</span>
                  </Show>
                </button>
                </>
              )}
            </For>
            <Show when={visible().length === 0}>
              <div class="px-3 py-2 text-xs text-muted-dark">No matches</div>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
};
