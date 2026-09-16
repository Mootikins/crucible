import { Accessor, Component, For, JSX, Setter, Show, Switch, Match, createMemo, createSignal } from 'solid-js';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import { ChatModeControl } from '@/components/ChatModeControl';

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

const kind = (chip: ComposerChip) => chip.render ?? (chip.options ? 'select' : 'static');

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
 */
export const ChipRow: Component<{ chips: ComposerChip[] }> = (props) => {
  // One slot per chip KEY. A surface rebuilds its list on every signal it
  // reads, so the objects are new each time; keyed by identity, `For` would
  // remount every chip on each rebuild and shut the popout the user just
  // opened. Keyed by index, a chip that appears mid-list (the workspace
  // target, once its provider answers) would shift every chip after it into
  // a neighbour's component. So `For` walks the KEYS, which are strings and
  // therefore stable, and each chip's data moves through its own signal.
  const slots = new Map<string, [Accessor<ComposerChip>, Setter<ComposerChip>]>();
  const keys = createMemo(
    () => {
      const seen = new Set<string>();
      for (const chip of props.chips) {
        const slot = slots.get(chip.key);
        if (slot) slot[1](chip);
        else slots.set(chip.key, createSignal(chip));
        seen.add(chip.key);
      }
      for (const key of [...slots.keys()]) if (!seen.has(key)) slots.delete(key);
      return props.chips.map((c) => c.key);
    },
    [] as string[],
    { equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]) },
  );

  return (
    <Show when={keys().length > 0}>
      <div class="mt-1.5 flex flex-wrap items-center gap-x-1 gap-y-1" data-testid="composer-chip-row">
        <For each={keys()}>{(key) => <ChipItem chip={slots.get(key)![0]()} />}</For>
      </div>
    </Show>
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
            label, not a control, and must not look like one. */}
        <span
          class="inline-flex items-center gap-0.5 px-2 py-1 rounded-md text-xs max-w-[220px] text-shell-body"
          title={chip().title ?? `${chip().label}: ${chip().value || chip().defaultLabel || ''}`}
          data-testid={chip().testid}
        >
          <Show when={chip().icon} keyed>
            {(Icon) => <Icon class="w-3.5 h-3.5 flex-shrink-0 text-muted-dark" />}
          </Show>
          <span class="truncate">{chip().value || chip().defaultLabel}</span>
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
