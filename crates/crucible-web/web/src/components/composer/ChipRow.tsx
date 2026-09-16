import {
  Accessor,
  Component,
  For,
  JSX,
  Setter,
  Show,
  Switch,
  Match,
  batch,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js';
import { Portal } from 'solid-js/web';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import { ChatModeControl } from '@/components/ChatModeControl';
import { placePopup, type PopupPlacement } from '@/lib/popup-placement';

type IconComponent = Component<{ class?: string }>;

/**
 * One entry on the composer's chip row: an axis (`label`), what it is set
 * to (`value`), and how it renders.
 *
 * `''` is always "nothing chosen". A chip that has a `defaultLabel` shows
 * what `''` will mean when the session starts, marked `· default`, so a
 * fresh draft reads as a list of facts and never as a list of blanks.
 */
export interface ComposerChip {
  /** 'model' | 'mode' | 'kiln' | 'project' | 'workspaceTarget' | 'runtime' | 'agent' | … */
  key: string;
  /** The axis, for screen readers and titles ("Model"). */
  label: string;
  /** '' when nothing is chosen. */
  value: string;
  /** What '' resolves to, shown as `<defaultLabel> · default`. */
  defaultLabel?: string;
  /**
   * How a CHOSEN value reads when its option's own label is not enough — a
   * target inside a provider submenu reads `Provider · Target`.
   */
  valueLabel?: string;
  /**
   * Where the chip sits on the row, and therefore how long it survives a
   * narrowing pane: LOWER shows first, and the last chips fold into the
   * `+N` button.
   *
   * The surface owns the ordering, not this component — only the surface
   * knows that a live session's model and mode outrank its status chips.
   * An entry without one sorts last, in the order the surface listed it.
   */
  priority?: number;
  /** Absent on a static fact chip. */
  options?: ChipOption[];
  onSelect?: (value: string) => void;
  icon?: IconComponent;
  disabled?: boolean;
  /** Every surface keeps its existing testids; the mode control owns its own. */
  testid?: string;
  /**
   * 'select' (the default when `options` exist), 'static' (the default when
   * they do not), 'mode' (the round mode control), or 'custom' (a chip whose
   * component the daemon drives — the plugin status chips).
   */
  render?: 'select' | 'mode' | 'static' | 'custom';
  /** A static chip's full text — the path behind a basename. */
  title?: string;
  /** Toggle-many (the live kiln set): picks toggle membership in `selected`. */
  multi?: boolean;
  selected?: string[];
  /** The picker extras a few chips need; passed straight to ChipSelect. */
  select?: {
    placeholder?: string;
    searchThreshold?: number;
    optionTestidPrefix?: string;
    create?: { label: (text: string) => string; run: (text: string) => void; when?: (text: string) => boolean };
    action?: {
      label: string;
      placeholder: string;
      buttonLabel: string;
      validate?: (text: string) => boolean;
      run: (text: string) => void;
    };
    footer?: JSX.Element;
  };
  /** The element a 'custom' chip draws. */
  element?: JSX.Element;
}

/** What the row needs to know to decide how many chips it can show. */
export interface ChipRowMeasurement {
  /** The row's own width, in px. */
  row: number;
  /** Each chip's natural width, in px, in the order the row draws them. */
  chips: number[];
}

const kind = (chip: ComposerChip) => chip.render ?? (chip.options ? 'select' : 'static');

/** `gap-x-1` between the chips, in px. */
const GAP = 4;
/** The `+N` button, `h-7 w-7` — the mode circle's size. */
const OVERFLOW_SIZE = 28;
/** Tallest the fold's list gets before it scrolls inside itself. */
const PANEL_MAX_HEIGHT = 340;
/** A chip with no stated priority sorts after every chip that has one. */
const LAST = Number.MAX_SAFE_INTEGER;

/**
 * The composer's chip row, built from data.
 *
 * Both composers — the new-session splash and the live session — draw this
 * one component under the same capsule. They differ only in the list they
 * hand it: the draft lists kiln, project, workspace, runtime, agent and
 * model; the live session lists model, mode, project, kiln and status. A
 * chip placed by hand on one surface used to drift from its twin on the
 * other (the draft drew its chips ABOVE the field, the session below), so
 * neither surface places a chip any more.
 *
 * The row sits UNDER the capsule and never inside it: chips in the field
 * made the field read as a toolbar.
 *
 * The row is ONE line and it never shrinks a chip. A chip that shrinks
 * loses its text — an ephemeral session's folder chip is a 36-character
 * id, and clipping it leaves a stub that names nothing. So the row shows
 * as many WHOLE chips as its width holds, in priority order, and folds the
 * rest into a `+N` button that lists them. What the width changes is the
 * COUNT, never a chip.
 */
export const ChipRow: Component<{
  chips: ComposerChip[];
  /**
   * The widths the fold is decided from. The default reads them off the
   * laid-out row; jsdom has no layout, so a test injects them instead and
   * states the split it expects.
   */
  measure?: () => ChipRowMeasurement;
}> = (props) => {
  // One slot per chip KEY. A surface rebuilds its list on every signal it
  // reads, so the objects are new each time; keyed by identity, `For` would
  // remount every chip on each rebuild and shut the popout the user just
  // opened. Keyed by index, a chip that appears mid-list (the workspace
  // target, once its provider answers) would shift every chip after it into
  // a neighbour's component. So `For` walks the KEYS, which are strings and
  // therefore stable, and each chip's data moves through its own signal.
  const slots = new Map<string, [Accessor<ComposerChip>, Setter<ComposerChip>]>();
  /** Each chip's wrapper element, while the row draws it. */
  const chipRefs = new Map<string, HTMLElement>();
  /**
   * Each chip's natural width, remembered from when it was last drawn.
   *
   * A folded chip is not in the document and has no width to read, so the
   * row cannot re-measure it. Drawing every chip on every pass to get one
   * back is what it did first, and that mounted and unmounted the whole row
   * — the fold button included — on each observer callback, in a loop that
   * the observer itself kept feeding. The cache breaks the loop: a full
   * pass runs only for a chip the row has never measured.
   */
  const widths = new Map<string, number>();
  // Priority decides the draw order. `sort` is stable, so chips that share a
  // priority — and the ones that state none — keep the surface's own order.
  const ordered = createMemo(() =>
    [...props.chips].sort((a, b) => (a.priority ?? LAST) - (b.priority ?? LAST)),
  );
  const keys = createMemo(
    () => {
      const seen = new Set<string>();
      for (const chip of ordered()) {
        const slot = slots.get(chip.key);
        if (slot) slot[1](chip);
        else slots.set(chip.key, createSignal(chip));
        seen.add(chip.key);
      }
      for (const key of [...slots.keys()]) {
        if (seen.has(key)) continue;
        slots.delete(key);
        // The element and the width go with the chip. A width left behind
        // would be spent on a chip that is no longer on the row.
        chipRefs.delete(key);
        widths.delete(key);
      }
      return ordered().map((c) => c.key);
    },
    [] as string[],
    { equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]) },
  );

  const chipAt = (key: string) => slots.get(key)![0]();

  /** How many chips the row draws. `Infinity` until a measurement says less. */
  const [shown, setShown] = createSignal(Number.POSITIVE_INFINITY);
  // Keys that measured zero wide. The live session's status chips draw
  // nothing until a plugin or a review gives them something to say, and a
  // chip that is not on the row must neither take a place on it nor appear
  // as a blank row inside the fold. They stay MOUNTED — invisible, and
  // therefore measurable again the moment they have something to draw.
  const [absent, setAbsent] = createSignal<string[]>([], {
    equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]),
  });
  // True for the one pass that draws every chip, so that every chip HAS a
  // width to read. The pass ends in a microtask, before any paint, so it
  // costs no frame and the user never sees it.
  const [measuring, setMeasuring] = createSignal(false);
  let rowRef: HTMLDivElement | undefined;
  let inMeasure = false;

  /** The chips that actually draw something, in priority order. */
  const present = () => {
    const gone = new Set(absent());
    return keys().filter((key) => !gone.has(key));
  };
  const drawn = () =>
    measuring() ? keys() : [...present().slice(0, shown()), ...absent()];
  // NOT gated on `measuring`. A full pass draws every chip for an instant,
  // and a fold button that unmounted for that instant was detached from the
  // document under the pointer — the click it was there to take went
  // nowhere. It keeps its count through the pass instead.
  const folded = () => present().slice(shown());

  /** How many whole chips `m.row` holds, leaving room for the button. */
  const fit = (m: ChipRowMeasurement) => {
    // No width to fit against (jsdom, a row not yet laid out, a detached
    // pane): show everything rather than nothing. An empty row is a worse
    // answer than a row that wraps for one frame.
    if (!(m.row > 0) || m.chips.length === 0) {
      batch(() => {
        setAbsent([]);
        setShown(Number.POSITIVE_INFINITY);
      });
      return;
    }
    const order = keys();
    const empty: string[] = [];
    const widths: number[] = [];
    m.chips.forEach((width, i) => {
      if (width > 0) widths.push(width);
      else if (order[i] !== undefined) empty.push(order[i]);
    });

    let used = 0;
    let n = 0;
    while (n < widths.length) {
      const width = widths[n] + (n > 0 ? GAP : 0);
      if (used + width > m.row) break;
      used += width;
      n += 1;
    }
    // The button is on the row too. Give chips back until it has room — a
    // button drawn past the edge hides the very chips it stands for.
    if (n < widths.length) {
      while (n > 0 && used + GAP + OVERFLOW_SIZE > m.row) {
        n -= 1;
        used -= widths[n] + (n > 0 ? GAP : 0);
      }
    }
    batch(() => {
      setAbsent(empty);
      setShown(n);
    });
  };

  const cache = (list: string[]) => {
    for (const key of list) {
      const el = chipRefs.get(key);
      if (el) widths.set(key, el.offsetWidth);
    }
  };

  const reading = (): ChipRowMeasurement => ({
    row: rowRef?.clientWidth ?? 0,
    chips: keys().map((key) => widths.get(key) ?? 0),
  });

  const remeasure = () => {
    if (props.measure) {
      fit(props.measure());
      return;
    }
    if (!rowRef || inMeasure) return;
    cache(drawn());
    if (keys().every((key) => widths.has(key))) {
      fit(reading());
      return;
    }
    // A chip the row has never drawn — the first pass, or one the surface
    // just added. Draw them ALL, then read their widths one microtask later.
    //
    // Not in this turn. `remeasure` runs from an effect as often as from the
    // observer, and Solid holds a signal written inside an effect until the
    // update cycle ends — so a width read on the next line comes off the
    // element the row drew BEFORE the pass, or off nothing at all. Zero
    // widths made every chip read as absent, which emptied the fold, which
    // unmounted the button under the user's pointer. A microtask is not a
    // frame: the DOM is in place and no paint has happened yet.
    inMeasure = true;
    setMeasuring(true);
    queueMicrotask(() => {
      cache(keys());
      setMeasuring(false);
      inMeasure = false;
      if (rowRef) fit(reading());
    });
  };

  onMount(() => {
    remeasure();
    if (!rowRef || typeof ResizeObserver === 'undefined') return;
    // The row for the width the split is decided against, and every chip for
    // its own: a status chip that arrives once the daemon answers changes no
    // row width at all, and would otherwise never be counted.
    const observer = new ResizeObserver(() => remeasure());
    observer.observe(rowRef);
    createEffect(() => {
      for (const key of drawn()) {
        const el = chipRefs.get(key);
        if (el) observer.observe(el);
      }
    });
    onCleanup(() => observer.disconnect());
  });

  // A chip whose text changed is a chip whose width changed, so the split has
  // to be decided again — a model id twice as long as the last one can push a
  // neighbour off the row without the pane moving at all. A chip that is
  // folded while its text changes is not in the document, so its remembered
  // width is now a lie: the cache goes, and the next pass takes them all.
  createEffect(() => {
    ordered()
      .map((c) => `${c.key} ${c.value} ${c.valueLabel ?? ''} ${c.defaultLabel ?? ''}`)
      .join('');
    widths.clear();
    remeasure();
  });

  return (
    <Show when={keys().length > 0}>
      <div
        ref={rowRef}
        class="mt-1.5 flex flex-nowrap items-center gap-x-1 overflow-hidden"
        data-testid="composer-chip-row"
      >
        <For each={drawn()}>
          {(key) => (
            <div class="flex shrink-0 items-center" ref={(el) => chipRefs.set(key, el)}>
              <ChipItem chip={chipAt(key)} />
            </div>
          )}
        </For>
        <Show when={folded().length > 0}>
          <ChipFold keys={folded()} chip={chipAt} />
        </Show>
      </div>
    </Show>
  );
};

/**
 * The chips the row had no room for, behind one round `+N` button.
 *
 * It opens on hover as well as on click, because it stands for facts the
 * user is reading rather than for an action — the same reason the mode
 * control's list opens on hover.
 */
const ChipFold: Component<{ keys: string[]; chip: (key: string) => ComposerChip }> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [pos, setPos] = createSignal<PopupPlacement | null>(null);
  let button: HTMLButtonElement | undefined;
  let panel: HTMLDivElement | undefined;

  const place = () => {
    if (!button) return;
    setPos(
      placePopup(button.getBoundingClientRect(), { width: window.innerWidth, height: window.innerHeight }, {
        width: Math.ceil(panel?.getBoundingClientRect().width || 240),
        preferredHeight: Math.ceil(panel?.getBoundingClientRect().height || PANEL_MAX_HEIGHT),
        gap: 4,
      }),
    );
  };

  // True while the list stands open because the pointer ARRIVED on the
  // button. A real pointer fires `mouseenter` before `click`, so a plain
  // toggle shut the list in the frame it appeared and the fold looked dead.
  // The click that follows the hover is absorbed; the one after it closes.
  let openedByHover = false;

  const show = () => {
    place();
    setOpen(true);
  };
  const close = () => {
    openedByHover = false;
    setOpen(false);
  };
  const onEnter = () => {
    if (open()) return;
    openedByHover = true;
    show();
  };
  const onClick = () => {
    if (openedByHover) {
      openedByHover = false;
      return;
    }
    if (open()) close();
    else show();
  };

  createEffect(() => {
    if (!open()) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      close();
    };
    const onDown = (e: MouseEvent) => {
      const target = e.target;
      if (!(target instanceof Node)) return;
      if (button?.contains(target) || panel?.contains(target)) return;
      // A chip inside this list opens its OWN popout, portaled elsewhere in
      // the document. A click there is still a click inside this control;
      // closing on it would unmount the picker mid-choice.
      if (target instanceof Element && target.closest('[data-chip-popout]')) return;
      close();
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    onCleanup(() => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    });
  });

  return (
    <>
      <button
        ref={button}
        type="button"
        aria-label={`${props.keys.length} more`}
        aria-expanded={open()}
        aria-haspopup="menu"
        title={`${props.keys.length} more`}
        data-testid="composer-chip-overflow"
        onClick={onClick}
        onMouseEnter={onEnter}
        classList={{
          // The mode circle's geometry: the same 28px height as the chips it
          // stands for, drawn as a circle and quiet until it is used.
          'focus-ring flex h-7 w-7 aspect-square shrink-0 items-center justify-center rounded-full text-xs leading-none transition-colors':
            true,
          'text-shell-body hover:bg-hover-wash': !open(),
          'bg-hover-wash text-shell-ink': open(),
        }}
      >
        +{props.keys.length}
      </button>

      <Show when={open()}>
        <Portal>
          <div
            ref={panel}
            data-chip-popout=""
            data-testid="composer-chip-overflow-popout"
            class="fixed z-50 flex flex-col min-w-[240px] max-w-[360px] overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 cru-anim-rise"
            style={{
              left: `${pos()?.left ?? 0}px`,
              ...(pos()?.top !== undefined
                ? { top: `${pos()!.top}px` }
                : { bottom: `${pos()?.bottom ?? 0}px` }),
              'max-height': `${pos()?.maxHeight ?? PANEL_MAX_HEIGHT}px`,
            }}
          >
            {/* Full rows, not a squeezed copy of the row above: this list is
                where a folded chip gets the width its text needs, so the axis
                is named beside it instead of being left to a title. */}
            <For each={props.keys}>
              {(key) => (
                <div class="flex items-center justify-between gap-3 px-2 py-0.5">
                  <span class="shrink-0 text-xs text-muted-dark">{props.chip(key).label}</span>
                  <ChipItem chip={props.chip(key)} />
                </div>
              )}
            </For>
          </div>
        </Portal>
      </Show>
    </>
  );
};

const ChipItem: Component<{ chip: ComposerChip }> = (props) => {
  const chip = () => props.chip;
  /** The trigger's text, or `undefined` to let ChipSelect look the option up. */
  const triggerLabel = () => {
    const c = chip();
    if (c.value === '' && c.defaultLabel) return c.defaultLabel;
    return c.valueLabel;
  };
  const showsDefault = () => chip().value === '' && !!chip().defaultLabel;

  return (
    <Switch>
      <Match when={kind(chip()) === 'mode'}>
        <ChatModeControl />
      </Match>
      <Match when={kind(chip()) === 'custom'}>{chip().element}</Match>
      <Match when={kind(chip()) === 'static'}>
        {/* Same footprint as a ChipSelect trigger, minus the caret: it is a
            label, not a control, and must not look like one. Its text is
            never clipped — the row folds whole chips instead. */}
        <span
          class="inline-flex items-center gap-0.5 px-2 py-1 rounded-md text-xs whitespace-nowrap text-shell-body"
          title={chip().title ?? `${chip().label}: ${chip().value || chip().defaultLabel || ''}`}
          data-testid={chip().testid}
        >
          <Show when={chip().icon} keyed>
            {(Icon) => <Icon class="w-3.5 h-3.5 flex-shrink-0 text-muted-dark" />}
          </Show>
          <span>{chip().value || chip().defaultLabel}</span>
        </span>
      </Match>
      <Match when={kind(chip()) === 'select'}>
        <ChipSelect
          name={chip().label.toLowerCase()}
          icon={chip().icon}
          options={chip().options ?? []}
          value={chip().value}
          onSelect={(v) => chip().onSelect?.(v)}
          disabled={chip().disabled}
          testid={chip().testid}
          triggerLabel={triggerLabel()}
          triggerHint={showsDefault() ? 'default' : undefined}
          truncate={false}
          multi={chip().multi}
          selected={chip().selected}
          placeholder={chip().select?.placeholder}
          searchThreshold={chip().select?.searchThreshold}
          optionTestidPrefix={chip().select?.optionTestidPrefix}
          create={chip().select?.create}
          action={chip().select?.action}
          footer={chip().select?.footer}
        />
      </Match>
    </Switch>
  );
};
