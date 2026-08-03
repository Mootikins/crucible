import { Component, For, JSX, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { ChevronDown, ChevronRight, Check } from '@/lib/icons';
import { placeFlyout, placePopup, type FlyoutPlacement } from '@/lib/popup-placement';
import { treeSectionHeader } from '@/components/tree/tree-style';

/** Tallest a chip popout gets before its list scrolls internally. */
const CHIP_PANEL_MAX_HEIGHT = 340;

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
  /**
   * Nested options, opened as a flyout beside this row (`Remote Machines ▸`).
   *
   * A row with children is a doorway, not a choice — clicking it opens the
   * flyout instead of selecting, so such a row needs no meaningful `value`.
   * Children are searched along with their parents, and a filter that matches
   * one surfaces it flattened under the parent's name: a target hidden behind
   * a closed flyout is otherwise unfindable by typing, which is the whole
   * reason the filter box exists.
   */
  children?: ChipOption[];
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
  /** Override the trigger's text outright (e.g. multi mode showing the anchor
   * item's name + "+N" instead of the default count badge). */
  triggerLabel?: string;
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
  const [panelPos, setPanelPos] = createSignal<{
    left: number;
    top?: number;
    bottom?: number;
    width?: number;
    maxHeight?: number;
  }>({ left: 0, top: 0 });
  const [actionMode, setActionMode] = createSignal(false);
  const [actionText, setActionText] = createSignal('');
  // Index into `visible()` of the row whose flyout is open, or -1.
  const [flyoutFor, setFlyoutFor] = createSignal(-1);
  const [flyoutHover, setFlyoutHover] = createSignal(-1);
  const [flyoutPos, setFlyoutPos] = createSignal<FlyoutPlacement | null>(null);
  let rootRef: HTMLDivElement | undefined;
  let panelRef: HTMLDivElement | undefined;
  let flyoutRef: HTMLDivElement | undefined;
  let triggerRef: HTMLButtonElement | undefined;
  let inputRef: HTMLInputElement | undefined;
  let actionInputRef: HTMLInputElement | undefined;

  /** Every selectable option, parents and children alike. */
  const flattened = (): ChipOption[] =>
    props.options.flatMap((o) => (o.children?.length ? o.children : [o]));

  const current = () => flattened().find((o) => o.value === props.value);
  const display = () => {
    if (props.triggerLabel !== undefined) return props.triggerLabel;
    if (props.multi) {
      const n = props.selected?.length ?? 0;
      return n > 0 ? `${props.name} · ${n}` : props.name;
    }
    // An empty value with an explicit placeholder reads as "nothing chosen"
    // even when a '' option exists in the list (e.g. the model chips' Auto
    // row) — the chip shows the placeholder, the list still offers the row.
    if (props.value === '' && props.placeholder) return props.placeholder;
    return current()?.label ?? props.placeholder ?? props.name;
  };
  const isPicked = (o: ChipOption) =>
    props.multi ? (props.selected ?? []).includes(o.value) : o.value === props.value;
  const searchable = () => flattened().length >= (props.searchThreshold ?? 8);
  const matches = (o: ChipOption, q: string) =>
    o.label.toLowerCase().includes(q) || !!o.hint?.toLowerCase().includes(q);

  const visible = (): ChipOption[] => {
    const q = filter().trim().toLowerCase();
    if (!q) return props.options;
    return props.options.flatMap((o) => {
      if (!o.children?.length) return matches(o, q) ? [o] : [];
      // A parent that matches offers all its children; otherwise only the
      // children that match themselves. Either way they arrive flattened and
      // grouped, because a flyout the filter cannot reach into hides them.
      const kids = o.children.filter((c) => matches(o, q) || matches(c, q));
      return kids.map((c) => ({ ...c, group: o.label }));
    });
  };

  const close = () => {
    setOpen(false);
    setFilter('');
    setHover(-1);
    setActionMode(false);
    setActionText('');
    closeFlyout();
  };

  const closeFlyout = () => {
    setFlyoutFor(-1);
    setFlyoutHover(-1);
  };

  /** The open flyout's children, or `[]` when none is open. */
  const flyoutOptions = (): ChipOption[] => visible()[flyoutFor()]?.children ?? [];

  /**
   * Open `row`'s flyout, anchored to the row element itself.
   *
   * Anchoring to the row rather than the panel is what keeps a long list's
   * submenus beside the row that owns them instead of all at the panel's top.
   */
  const openFlyout = (index: number, row: HTMLElement) => {
    const rect = row.getBoundingClientRect();
    setFlyoutPos(
      placeFlyout(rect, { width: window.innerWidth, height: window.innerHeight }, {
        width: 220,
        preferredHeight: CHIP_PANEL_MAX_HEIGHT,
      }),
    );
    setFlyoutFor(index);
    setFlyoutHover(-1);
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

  /**
   * A row's click: open its flyout when it has one, otherwise select it.
   *
   * A parent row carries no selectable value of its own, so selecting it would
   * hand the caller a value naming a category rather than a target.
   */
  const activate = (o: ChipOption, index: number, row: HTMLElement) => {
    if (o.disabled) return;
    if (o.children?.length) {
      if (flyoutFor() === index) closeFlyout();
      else openFlyout(index, row);
      return;
    }
    pick(o);
  };

  /**
   * Position the panel against the trigger.
   *
   * `panelWidth`/`panelHeight` come from the rendered panel when available —
   * the first pass has nothing to measure, so it falls back to the CSS
   * min-width and lets the follow-up pass correct it. Measuring matters
   * because the panel is content-sized and wider than its chip: without the
   * clamp, a chip near the right edge (narrow window, docked panel) pushed
   * the popout off-screen.
   */
  const positionPanel = () => {
    if (!triggerRef) return;
    const rect = triggerRef.getBoundingClientRect();
    const panel = panelRef?.getBoundingClientRect();
    setPanelPos(
      placePopup(rect, { width: window.innerWidth, height: window.innerHeight }, {
        // ceil, not round: a 221.25px panel rounded down spills a quarter
        // pixel past the clamp.
        width: Math.ceil(panel?.width || 220),
        preferredHeight: Math.ceil(panel?.height || CHIP_PANEL_MAX_HEIGHT),
        gap: 4,
      }),
    );
  };

  const openPopout = () => {
    positionPanel();
    setOpen(true);
    props.onOpen?.();
  };

  // Re-place once the panel exists and its real size is known. rAF, not a
  // microtask: the correction needs the panel's laid-out width, and a
  // microtask runs before layout — leaving the first pass's 220px estimate in
  // place for panels that render wider.
  createEffect(() => {
    if (!open()) return;
    requestAnimationFrame(positionPanel);
  });

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
      // The panel and its flyout are portaled out of rootRef's subtree — a
      // click inside either is inside the control.
      if (
        rootRef &&
        !rootRef.contains(t) &&
        panelRef &&
        !panelRef.contains(t) &&
        !flyoutRef?.contains(t)
      )
        close();
    };
    const onKey = (e: KeyboardEvent) => {
      // The action-mode input owns its keys (Escape exits the mode, Enter
      // submits) — the list-navigation handler must not also close the
      // popout or pick an option on the same event.
      if (actionInputRef && e.target === actionInputRef) return;
      const inFlyout = flyoutFor() >= 0;
      if (e.key === 'Escape') {
        e.stopPropagation();
        // Escape backs out one level: a submenu opened by mistake should not
        // also discard the selection the user came here to make.
        if (inFlyout) closeFlyout();
        else close();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const delta = e.key === 'ArrowDown' ? 1 : -1;
        if (inFlyout) {
          const kids = flyoutOptions();
          if (kids.length) setFlyoutHover((h) => (h + delta + kids.length) % kids.length);
          return;
        }
        const list = visible();
        if (!list.length) return;
        setHover((h) => (h + delta + list.length) % list.length);
      } else if (e.key === 'ArrowRight' && !inFlyout) {
        const index = hover();
        const row = visible()[index];
        if (row?.children?.length && panelRef) {
          e.preventDefault();
          const el = panelRef.querySelectorAll('[role="option"]')[index];
          if (el instanceof HTMLElement) openFlyout(index, el);
        }
      } else if (e.key === 'ArrowLeft' && inFlyout) {
        e.preventDefault();
        closeFlyout();
      } else if (e.key === 'Enter') {
        if (inFlyout) {
          const kid = flyoutOptions()[flyoutHover()];
          if (kid) {
            e.preventDefault();
            pick(kid);
          }
          return;
        }
        const o = visible()[hover()] ?? visible()[0];
        if (o) {
          e.preventDefault();
          if (o.children?.length && panelRef) {
            const el = panelRef.querySelectorAll('[role="option"]')[visible().indexOf(o)];
            if (el instanceof HTMLElement) openFlyout(visible().indexOf(o), el);
          } else {
            pick(o);
          }
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
            class="fixed z-50 min-w-[220px] max-w-[320px] overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 cru-anim-rise"
            style={{
              left: `${panelPos().left}px`,
              ...(panelPos().top !== undefined
                ? { top: `${panelPos().top}px` }
                : { bottom: `${panelPos().bottom}px` }),
              // Long lists (many kilns/models) scroll inside the panel rather
              // than running past the viewport edge. `??`, not `||`: only the
              // pre-open default omits maxHeight, and a measured 0 (nothing
              // fits either side) must survive as 0.
              'max-height': `${panelPos().maxHeight ?? CHIP_PANEL_MAX_HEIGHT}px`,
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
                      onMouseEnter={(e) => {
                        setHover(i());
                        // Hovering a plain row dismisses a flyout left open by
                        // a sibling — two open submenus would both look live.
                        if (o.children?.length) openFlyout(i(), e.currentTarget);
                        else if (flyoutFor() !== -1) closeFlyout();
                      }}
                      onClick={(e) => activate(o, i(), e.currentTarget)}
                      aria-haspopup={o.children?.length ? 'menu' : undefined}
                      aria-expanded={o.children?.length ? flyoutFor() === i() : undefined}
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
                      <Show when={o.children?.length}>
                        <ChevronRight
                          class={`w-3.5 h-3.5 flex-shrink-0 text-muted-dark ${o.hint ? '' : 'ml-auto'}`}
                        />
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

        {/* A second Portal, not a child of the panel: the panel scrolls and
            clips its own overflow, so a flyout nested inside it would be cut
            off at the panel edge — the same reason the panel itself is
            portaled out of the composer. */}
        <Show when={flyoutFor() >= 0 && flyoutPos()} keyed>
          {(pos) => (
            <Portal>
              <div
                ref={flyoutRef}
                role="menu"
                aria-label={visible()[flyoutFor()]?.label}
                data-testid={props.testid ? `${props.testid}-flyout` : undefined}
                class="fixed z-50 min-w-[220px] max-w-[320px] overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 cru-anim-rise"
                style={{
                  left: `${pos.left}px`,
                  top: `${pos.top}px`,
                  'max-height': `${pos.maxHeight}px`,
                }}
              >
                <For each={flyoutOptions()}>
                  {(child, i) => (
                    <button
                      type="button"
                      role="option"
                      aria-selected={isPicked(child)}
                      disabled={child.disabled}
                      onMouseEnter={() => setFlyoutHover(i())}
                      onClick={() => pick(child)}
                      data-testid={
                        props.optionTestidPrefix
                          ? `${props.optionTestidPrefix}-${child.value}`
                          : undefined
                      }
                      classList={{
                        'w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors': true,
                        'bg-hover-wash': flyoutHover() === i(),
                        'text-shell-ink': !child.disabled,
                        'text-muted-dark cursor-not-allowed': !!child.disabled,
                      }}
                    >
                      <Show when={child.icon} keyed>
                        {(Icon) => <Icon class="w-3.5 h-3.5 flex-shrink-0 text-muted-dark" />}
                      </Show>
                      <span class="truncate">{child.label}</span>
                      <Show when={isPicked(child)}>
                        <Check class="w-3.5 h-3.5 flex-shrink-0 text-primary" />
                      </Show>
                      <Show when={child.hint}>
                        <span class="ml-auto pl-3 text-muted-dark truncate max-w-[140px]">
                          {child.hint}
                        </span>
                      </Show>
                    </button>
                  )}
                </For>
                <Show when={flyoutOptions().length === 0}>
                  <div class="px-3 py-2 text-xs text-muted-dark">Nothing here yet</div>
                </Show>
              </div>
            </Portal>
          )}
        </Show>
      </Show>
    </div>
  );
};
