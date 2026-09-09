import { Component, For, Match, Show, Switch } from 'solid-js';
import type { JSX } from 'solid-js';
import type { OilTree } from '@/lib/oil-types';

/**
 * Draws a serialized Oil node tree — the same tree the TUI renders to cells.
 *
 * The contract mirrors `PluginSettings.tsx`: a node arrives externally tagged
 * (`{"box": {...}}`, `{"text": {...}}`) and this switches on the tag alone, so
 * a plugin shipped tomorrow renders here with no change. The shape is pinned
 * on the Rust side by `crates/crucible-oil/tests/wire_shape.rs`; read that
 * before changing anything here.
 *
 * An unrecognised tag renders as a visible placeholder rather than nothing,
 * for the reason the settings pane gives: a construct added after this file
 * was written should look plain, not invisible.
 *
 * ## What does not survive the crossing
 *
 * Oil measures in terminal cells and paints with the sixteen terminal colours.
 * Neither exists in a browser, so both are mapped, and the mapping is a real
 * loss of authority rather than a detail:
 *
 *   - **Cells become a spacing scale.** One cell of padding is one step, not a
 *     character width. A tree tuned to look right at 80 columns has no way to
 *     say so here.
 *   - **Colours become theme tokens.** `red` is the palette's red, not the
 *     user's terminal red. `Indexed` and `Rgb` have no token to map to, so
 *     they pass through literally and ignore the theme.
 *
 * A plugin that wants pixel control does not get it from this vocabulary. That
 * is the trade the vocabulary makes, and it is the argument for it: the same
 * declaration draws in both places precisely because neither place lets the
 * plugin reach past it.
 */

/** One terminal cell, as a browser length. */
const CELL = '0.375rem';

/** The sixteen names, mapped onto tokens that already follow the theme. */
const COLORS: Record<string, string> = {
  black: 'var(--color-shell-bg)',
  red: 'var(--color-danger, #d9534f)',
  green: 'var(--color-ok, #4c9a5a)',
  yellow: 'var(--color-warn, #c99a2e)',
  blue: 'var(--color-info, #4a7fb5)',
  magenta: 'var(--color-precog, #9a6ab5)',
  cyan: 'var(--color-accent, #3d9caa)',
  white: 'var(--color-shell-ink)',
  gray: 'var(--color-muted)',
  dark_gray: 'var(--color-muted)',
  bright_red: 'var(--color-danger, #ef7370)',
  bright_green: 'var(--color-ok, #6fbf7f)',
  bright_yellow: 'var(--color-warn, #e6bb55)',
  bright_blue: 'var(--color-info, #6ea5d9)',
  bright_magenta: 'var(--color-precog, #b98fd1)',
  bright_cyan: 'var(--color-accent, #5fc0cd)',
  bright_white: 'var(--color-shell-ink)',
  reset: 'inherit',
};

export type { OilTree };

interface OilStyle {
  fg?: unknown;
  bg?: unknown;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  dim?: boolean;
  reverse?: boolean;
}

/**
 * `Color` is a name (`"red"`), or an object for the two variants that carry
 * data (`{"indexed": 12}`, `{"rgb": [r, g, b]}`).
 */
function color(value: unknown): string | undefined {
  if (typeof value === 'string') return COLORS[value] ?? undefined;
  if (value && typeof value === 'object') {
    const rgb = (value as { rgb?: [number, number, number] }).rgb;
    if (Array.isArray(rgb)) return `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]})`;
    // An indexed colour is a slot in the user's terminal palette. A browser
    // has no such palette, so it inherits rather than guessing at xterm's.
    return undefined;
  }
  return undefined;
}

function textStyle(s: OilStyle | undefined): JSX.CSSProperties {
  if (!s) return {};
  return {
    color: color(s.fg),
    'background-color': color(s.bg),
    'font-weight': s.bold ? '600' : undefined,
    'font-style': s.italic ? 'italic' : undefined,
    'text-decoration': s.underline ? 'underline' : undefined,
    opacity: s.dim ? 0.65 : undefined,
  };
}

function cells(n: unknown): string | undefined {
  const v = typeof n === 'number' ? n : 0;
  return v > 0 ? `calc(${CELL} * ${v})` : undefined;
}

interface Edges {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}

function edges(p: Edges | undefined): string | undefined {
  if (!p) return undefined;
  const v = (n: number | undefined) => `calc(${CELL} * ${n ?? 0})`;
  if (!p.top && !p.right && !p.bottom && !p.left) return undefined;
  return `${v(p.top)} ${v(p.right)} ${v(p.bottom)} ${v(p.left)}`;
}

const JUSTIFY: Record<string, string> = {
  start: 'flex-start',
  end: 'flex-end',
  center: 'center',
  space_between: 'space-between',
  space_around: 'space-around',
  space_evenly: 'space-evenly',
};

const ALIGN: Record<string, string> = {
  start: 'flex-start',
  end: 'flex-end',
  center: 'center',
  stretch: 'stretch',
};

interface BoxNode {
  children?: OilTree[];
  direction?: string;
  size?: unknown;
  padding?: Edges;
  margin?: Edges;
  border?: unknown;
  style?: OilStyle;
  justify?: string;
  align?: string;
  gap?: { row?: number; column?: number };
}

function boxStyle(b: BoxNode): JSX.CSSProperties {
  const row = b.direction === 'row';
  // Every border style is one hairline here. The four Oil borders differ by
  // which box-drawing characters they use, which is a distinction a browser
  // cannot make and should not fake with four weights.
  const bordered = b.border !== undefined && b.border !== null;
  const gap = row ? b.gap?.column : b.gap?.row;
  return {
    display: 'flex',
    'flex-direction': row ? 'row' : 'column',
    gap: cells(gap),
    padding: edges(b.padding),
    margin: edges(b.margin),
    border: bordered ? '1px solid var(--color-hairline)' : undefined,
    'border-radius': bordered ? '4px' : undefined,
    'justify-content': b.justify ? JUSTIFY[b.justify] : undefined,
    'align-items': b.align ? ALIGN[b.align] : 'stretch',
    ...textStyle(b.style),
    // A Flex node takes its share of the main axis; Content and Fixed do not.
    // Fixed is a cell count, which is why it maps to a basis and not a width.
    ...flexSize(b.size),
  };
}

function flexSize(size: unknown): JSX.CSSProperties {
  if (!size || typeof size !== 'object') return {};
  const flex = (size as { flex?: number }).flex;
  if (typeof flex === 'number') return { flex: `${flex} 1 0%` };
  const fixed = (size as { fixed?: number }).fixed;
  if (typeof fixed === 'number') return { 'flex-basis': cells(fixed), 'flex-shrink': 0 };
  return {};
}

export interface OilNodeProps {
  node: OilTree;
  /** Called when an `action` node is activated. */
  onAction?: (action: string, params: Record<string, string>) => void;
}

export const OilNode: Component<OilNodeProps> = (props) => {
  // `Node::Empty` is a unit variant, so it arrives as a bare string.
  const tag = () => (typeof props.node === 'string' ? props.node : Object.keys(props.node)[0]);
  const body = () => (typeof props.node === 'string' ? {} : (props.node as Record<string, any>)[tag()]);

  return (
    <Switch fallback={<UnknownNode tag={tag()} />}>
      <Match when={tag() === 'empty'}>{null}</Match>

      <Match when={tag() === 'text'}>
        <span style={textStyle(body()?.style)} class="whitespace-pre-wrap">
          {body()?.content ?? ''}
        </span>
      </Match>

      <Match when={tag() === 'box'}>
        <div style={boxStyle(body() as BoxNode)}>
          <For each={(body()?.children ?? []) as OilTree[]}>
            {(child) => <OilNode node={child} onAction={props.onAction} />}
          </For>
        </div>
      </Match>

      {/* Fragment and Slot are transparent containers in Oil too. */}
      <Match when={tag() === 'fragment' || tag() === 'slot'}>
        <For each={(tag() === 'fragment' ? body() : body()?.children) ?? []}>
          {(child: OilTree) => <OilNode node={child} onAction={props.onAction} />}
        </For>
      </Match>

      <Match when={tag() === 'action'}>
        <ActionNode body={body()} onAction={props.onAction} />
      </Match>

      <Match when={tag() === 'spinner'}>
        <span class="text-muted italic">{body()?.label ?? 'working…'}</span>
      </Match>

      <Match when={tag() === 'input'}>
        {/* Read-only: an Oil input is driven by the TUI's key handling, and a
            view has no way yet to send a typed value back. Rendering it as a
            live field would promise an edit that goes nowhere. */}
        <span class="px-1 rounded border border-hairline bg-control text-muted">
          {body()?.value || body()?.placeholder || ''}
        </span>
      </Match>

      <Match when={tag() === 'overlay'}>
        <OilNode node={body()?.child} onAction={props.onAction} />
      </Match>

      <Match when={tag() === 'raw'}>
        {/* Raw is a terminal escape sequence. There is nothing to show. */}
        {null}
      </Match>
    </Switch>
  );
};

const ActionNode: Component<{
  body: { child?: OilTree; action?: string; params?: Record<string, string> };
  onAction?: (action: string, params: Record<string, string>) => void;
}> = (props) => {
  const fire = () => props.onAction?.(props.body.action ?? '', props.body.params ?? {});
  return (
    <button
      type="button"
      class="text-left cursor-pointer bg-transparent border-0 p-0 font-inherit
             hover:opacity-80 focus:outline-none focus-visible:ring-1 focus-visible:ring-primary rounded"
      onClick={fire}
    >
      <Show when={props.body.child}>
        {(child) => <OilNode node={child()} onAction={props.onAction} />}
      </Show>
    </button>
  );
};

const UnknownNode: Component<{ tag: string }> = (props) => (
  <span class="text-muted italic" title={`No renderer for the Oil node '${props.tag}'`}>
    [{props.tag}]
  </span>
);

export default OilNode;
