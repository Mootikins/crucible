import { Component, For, JSX, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { ChevronDown, Check } from '@/lib/icons';
import { treeSectionHeader } from '@/components/tree/tree-style';

export interface ChipOption {
  value: string;
  label: string;
  /** Dimmed suffix (e.g. a path or availability note). */
  hint?: string;
  disabled?: boolean;
  /** Section header rendered above this option when it differs from the
   * previous option's group (caller keeps options sorted by group). */
  group?: string;
  /** Leading icon on the row (Cursor's menus icon every row). */
  icon?: Component<{ class?: string }>;
}

/**
 * Context chip with a custom popout list — the composer's picker idiom
 * (repo/branch-switcher style): a quiet inline `label ˅` trigger, a floating
 * panel with the options, a filter box once the list is big enough to need
 * one, and a check mark on the current selection. Native <select> can't do
 * any of that (no search, no hints, no styling of the list).
 *
 * The panel renders through a Portal with fixed positioning: triggers live
 * inside edge panels whose animation frame is `overflow-hidden` and whose
 * translate creates a stacking context — an in-place absolute popout gets
 * clipped at the panel edge and painted UNDER the center area, and no
 * z-index can save it.
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
  /** Replace the default quiet-chip trigger classes (e.g. a bordered field). */
  triggerClass?: string;
  /** Called when the popout opens (lazy data loads hook in here). */
  onOpen?: () => void;
  /** Offer creating from the filter text when it matches no option exactly
   * (repo/branch-switcher "create …" row). `when` further gates on the text
   * shape (e.g. only git URLs). */
  create?: {
    label: (text: string) => string;
    run: (text: string) => void;
    when?: (text: string) => boolean;
  };
  /** Toggle-many mode (facet filters): picks toggle membership in `selected`
   * and the popout stays open; the trigger shows a count badge. */
  multi?: boolean;
  selected?: string[];
  /** Always-visible footer action (Cursor's "Add repo" idiom): a labeled row
   * that flips inline into an input + confirm button. Unlike `create`, this
   * is DISCOVERABLE — it does not require typing into the filter first. */
  action?: {
    label: string;
    placeholder: string;
    buttonLabel: string;
    validate?: (text: string) => boolean;
    run: (text: string) => void;
  };
  /** Detached card under the list (status/info, e.g. a settings note). */
  footer?: JSX.Element;
  /** Per-row data-testids: `${prefix}-${option.value}`. */
  optionTestidPrefix?: string;
}> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [filter, setFilter] = createSignal('');
  const [hover, setHover] = createSignal(-1);
  const [panelPos, setPanelPos] = createSignal<{ left: number; top?: number; bottom?: number }>({
    left: 0,
    top: 0,
  });
  const [actionMode, setActionMode] = createSignal(false);
  const [actionText, setActionText] = createSignal('');
  let rootRef: HTMLDivElement | undefined;
  let panelRef: HTMLDivElement | undefined;
  let triggerRef: HTMLButtonElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let actionInputRef: HTMLInputElement | undefined;

  const current = () => props.options.find((o) => o.value === props.value);
  const display = () => {
    if (props.multi) {
      const n = props.selected?.length ?? 0;
      return n > 0 ? `${props.name} · ${n}` : props.name;
    }
    return current()?.label ?? props.placeholder ?? props.name;
  };
  const isPicked = (o: ChipOption) =>
    props.multi ? (props.selected ?? []).includes(o.value) : o.value === props.value;
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
    setActionMode(false);
    setActionText('');
  };

  const actionValid = () => {
    const text = actionText().trim();
    return !!text && (!props.action?.validate || props.action.validate(text));
  };

  const runAction = () => {
    if (!props.action || !actionValid()) return;
    props.action.run(actionText().trim());
    close();
  };

  const pick = (o: ChipOption) => {
    if (o.disabled) return;
    props.onSelect(o.value);
    if (!props.multi) close();
  };

  const openPopout = () => {
    if (triggerRef) {
      const rect = triggerRef.getBoundingClientRect();
      // Anchor upward when the trigger sits near the viewport bottom (the
      // chat input) — a downward panel would fall off-screen.
      const spaceBelow = window.innerHeight - rect.bottom;
      setPanelPos(
        spaceBelow < 340
          ? { left: Math.round(rect.left), bottom: Math.round(window.innerHeight - rect.top + 4) }
          : { left: Math.round(rect.left), top: Math.round(rect.bottom + 4) },
      );
    }
    setOpen(true);
    props.onOpen?.();
  };

  // The "create '<text>'" row appears once the filter text stops matching any
  // option exactly — picking it hands the raw text to the caller.
  const createText = () => {
    if (!props.create) return null;
    const text = filter().trim();
    if (!text) return null;
    if (props.create.when && !props.create.when(text)) return null;
    return props.options.some((o) => o.label === text || o.value === text) ? null : text;
  };

  const runCreate = () => {
    const text = createText();
    if (!text || !props.create) return;
    props.create.run(text);
    close();
  };

  createEffect(() => {
    if (!open()) return;
    queueMicrotask(() => inputRef?.focus());
    const onDocClick = (e: MouseEvent) => {
      const t = e.target as Node;
      // The panel is portaled out of rootRef's subtree — check both.
      if (rootRef && !rootRef.contains(t) && panelRef && !panelRef.contains(t)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      // The action-mode input owns its keys (Escape exits the mode, Enter
      // submits) — the list-navigation handler must not also close the
      // popout or pick an option on the same event.
      if (actionInputRef && e.target === actionInputRef) return;
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
        } else if (createText()) {
          e.preventDefault();
          runCreate();
        }
      }
    };
    // A viewport change moves the trigger out from under the fixed panel —
    // close instead of chasing it. Scrolls INSIDE the panel (the options
    // list) are fine; only background scrolls detach the trigger.
    const onViewportChange = () => close();
    const onScroll = (e: Event) => {
      if (panelRef && e.target instanceof Node && panelRef.contains(e.target)) return;
      close();
    };
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
    window.addEventListener('resize', onViewportChange);
    window.addEventListener('scroll', onScroll, { capture: true });
    onCleanup(() => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKey);
      window.removeEventListener('resize', onViewportChange);
      window.removeEventListener('scroll', onScroll, { capture: true });
    });
  });

  return (
    <div ref={rootRef} class="relative inline-flex">
      <button
        ref={triggerRef}
        type="button"
        aria-label={props.name}
        aria-expanded={open()}
        disabled={props.disabled}
        data-testid={props.testid}
        onClick={() => (open() ? close() : openPopout())}
        classList={{
          [props.triggerClass ??
          'inline-flex items-center gap-1 px-2 py-1 rounded-md text-xs transition-colors max-w-[220px]']: true,
          'text-shell-body hover:bg-hover-wash': !props.triggerClass && !open(),
          'bg-hover-wash text-shell-ink': !props.triggerClass && open(),
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
        <Portal>
          <div
            ref={panelRef}
            data-testid={props.testid ? `${props.testid}-popout` : undefined}
            class="fixed z-50 min-w-[220px] max-w-[320px] bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 cru-anim-rise"
            style={{
              left: `${panelPos().left}px`,
              ...(panelPos().top !== undefined
                ? { top: `${panelPos().top}px` }
                : { bottom: `${panelPos().bottom}px` }),
            }}
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
                      <div class={treeSectionHeader}>{o.group}</div>
                    </Show>
                    <button
                      type="button"
                      role="option"
                      aria-selected={isPicked(o)}
                      disabled={o.disabled}
                      onMouseEnter={() => setHover(i())}
                      onClick={() => pick(o)}
                      data-testid={
                        props.optionTestidPrefix
                          ? `${props.optionTestidPrefix}-${o.value}`
                          : undefined
                      }
                      classList={{
                        'w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors': true,
                        'bg-hover-wash': hover() === i(),
                        'text-shell-ink': !o.disabled,
                        'text-muted-dark cursor-not-allowed': !!o.disabled,
                      }}
                    >
                      <Show when={o.icon} keyed>
                        {(Icon) => <Icon class="w-3.5 h-3.5 flex-shrink-0 text-muted-dark" />}
                      </Show>
                      <span class="truncate">{o.label}</span>
                      <Show when={isPicked(o)}>
                        <Check class="w-3.5 h-3.5 flex-shrink-0 text-primary" />
                      </Show>
                      <Show when={o.hint}>
                        <span class="ml-auto pl-3 text-muted-dark truncate max-w-[140px]">
                          {o.hint}
                        </span>
                      </Show>
                    </button>
                  </>
                )}
              </For>
              <Show when={visible().length === 0 && !createText()}>
                <div class="px-3 py-2 text-xs text-muted-dark">No matches</div>
              </Show>
              <Show when={createText()} keyed>
                {(text) => (
                  <button
                    type="button"
                    onClick={runCreate}
                    class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs text-primary hover:bg-hover-wash transition-colors border-t border-hairline"
                    data-testid={props.testid ? `${props.testid}-create` : undefined}
                  >
                    <span class="w-3.5 flex-shrink-0">＋</span>
                    <span class="truncate">{props.create!.label(text)}</span>
                  </button>
                )}
              </Show>
            </div>
            <Show when={props.action}>
              <Show
                when={actionMode()}
                fallback={
                  <button
                    type="button"
                    onClick={() => {
                      setActionMode(true);
                      queueMicrotask(() => actionInputRef?.focus());
                    }}
                    class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs text-primary hover:bg-hover-wash transition-colors border-t border-hairline"
                    data-testid={props.testid ? `${props.testid}-action` : undefined}
                  >
                    <span class="w-3.5 flex-shrink-0">＋</span>
                    <span class="truncate">{props.action!.label}</span>
                  </button>
                }
              >
                <div class="px-2 py-1.5 border-t border-hairline flex items-center gap-1.5">
                  <input
                    ref={actionInputRef}
                    value={actionText()}
                    onInput={(e) => setActionText(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Escape') {
                        setActionMode(false);
                        setActionText('');
                      } else if (e.key === 'Enter') {
                        e.preventDefault();
                        runAction();
                      }
                    }}
                    placeholder={props.action!.placeholder}
                    aria-label={props.action!.label}
                    class="flex-1 min-w-0 bg-control text-xs text-shell-ink placeholder-muted-dark rounded border border-hairline focus:border-primary outline-none px-2 py-1"
                    data-testid={props.testid ? `${props.testid}-action-input` : undefined}
                  />
                  <button
                    type="button"
                    onClick={runAction}
                    disabled={!actionValid()}
                    class="text-xs px-2 py-1 rounded bg-primary/15 text-primary border border-primary/40 hover:bg-primary/25 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex-shrink-0"
                    data-testid={props.testid ? `${props.testid}-action-submit` : undefined}
                  >
                    {props.action!.buttonLabel}
                  </button>
                </div>
              </Show>
            </Show>
            <Show when={props.footer}>{props.footer}</Show>
          </div>
        </Portal>
      </Show>
    </div>
  );
};
