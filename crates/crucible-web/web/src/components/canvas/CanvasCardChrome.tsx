import { Component, For, Show, createSignal } from 'solid-js';
import { Palette, Pencil, Trash2 } from '@/lib/icons';
import { resolveCanvasColor, type CanvasColor } from '@/lib/canvas-types';

/**
 * The affordances around a selected card.
 *
 * Three rules, all learned by getting them wrong first:
 *
 * - **Resize is cursor-only.** Painted grab boxes at the corners read as
 *   clutter and fight with the card's own border for attention. The hit zones
 *   are still there, they just announce themselves by changing the cursor.
 * - **Corners resize, edge midpoints connect.** Separate gestures never
 *   compete for the same pixels.
 * - **One button per intent, nested.** A row of seven colour swatches is a
 *   palette permanently open on every selection; the palette now lives behind
 *   a single button, alongside edit and delete.
 */

export type ResizeCorner = 'nw' | 'ne' | 'sw' | 'se';
export type EdgeSide = 'top' | 'right' | 'bottom' | 'left';

const CORNERS: { corner: ResizeCorner; class: string; cursor: string }[] = [
  { corner: 'nw', class: '-left-1.5 -top-1.5', cursor: 'nwse-resize' },
  { corner: 'ne', class: '-right-1.5 -top-1.5', cursor: 'nesw-resize' },
  { corner: 'sw', class: '-left-1.5 -bottom-1.5', cursor: 'nesw-resize' },
  { corner: 'se', class: '-right-1.5 -bottom-1.5', cursor: 'nwse-resize' },
];

const SIDES: { side: EdgeSide; zone: string; dot: string }[] = [
  {
    side: 'top',
    zone: 'left-1/2 -translate-x-1/2 -top-2 h-4 w-12',
    dot: 'left-1/2 -translate-x-1/2 -top-1.5',
  },
  {
    side: 'bottom',
    zone: 'left-1/2 -translate-x-1/2 -bottom-2 h-4 w-12',
    dot: 'left-1/2 -translate-x-1/2 -bottom-1.5',
  },
  {
    side: 'left',
    zone: 'top-1/2 -translate-y-1/2 -left-2 w-4 h-12',
    dot: 'top-1/2 -translate-y-1/2 -left-1.5',
  },
  {
    side: 'right',
    zone: 'top-1/2 -translate-y-1/2 -right-2 w-4 h-12',
    dot: 'top-1/2 -translate-y-1/2 -right-1.5',
  },
];

/** The six spec presets, plus "no colour". */
export const SWATCHES: (CanvasColor | undefined)[] = [undefined, '1', '2', '3', '4', '5', '6'];

/** Shared popover shell so card and edge toolbars are visually identical. */
export const ToolbarShell: Component<{ children: unknown; testid: string }> = (props) => (
  <div
    class="absolute -top-10 left-1/2 z-30 flex -translate-x-1/2 items-center gap-0.5 rounded-md border border-hairline bg-surface-elevated px-1 py-1 shadow-md"
    style={{ 'pointer-events': 'auto' }}
    data-testid={props.testid}
    onPointerDown={(e) => e.stopPropagation()}
    onDblClick={(e) => e.stopPropagation()}
  >
    {props.children as never}
  </div>
);

export const ToolbarButton: Component<{
  title: string;
  testid?: string;
  active?: boolean;
  onClick: () => void;
  children: unknown;
}> = (props) => (
  <button
    type="button"
    class="rounded p-1 text-muted hover:bg-hover-wash hover:text-shell-ink"
    classList={{ 'bg-hover-wash text-shell-ink': props.active }}
    title={props.title}
    data-testid={props.testid}
    onClick={() => props.onClick()}
  >
    {props.children as never}
  </button>
);

export const SwatchRow: Component<{
  color?: CanvasColor;
  onColor: (color: CanvasColor | undefined) => void;
}> = (props) => (
  <div class="flex items-center gap-1 pl-1" data-testid="canvas-palette">
    <For each={SWATCHES}>
      {(swatch) => (
        <button
          type="button"
          class="h-4 w-4 rounded-full border border-hairline transition-transform hover:scale-110"
          classList={{ 'ring-1 ring-primary': (props.color ?? undefined) === swatch }}
          style={{ background: resolveCanvasColor(swatch) ?? 'transparent' }}
          title={swatch ? `Colour ${swatch}` : 'No colour'}
          data-testid="canvas-color-swatch"
          onClick={() => props.onColor(swatch)}
        />
      )}
    </For>
  </div>
);

export const CanvasCardChrome: Component<{
  selected: boolean;
  /** Suppressed while editing — handles would fight with text selection. */
  editing: boolean;
  color?: CanvasColor;
  /** Groups and media have nothing to edit as text. */
  canEdit: boolean;
  onResizeStart: (e: PointerEvent, corner: ResizeCorner) => void;
  onConnectStart: (e: PointerEvent, side: EdgeSide) => void;
  onColor: (color: CanvasColor | undefined) => void;
  onEdit: () => void;
  onDelete: () => void;
}> = (props) => {
  const [paletteOpen, setPaletteOpen] = createSignal(false);

  return (
    <Show when={props.selected && !props.editing}>
      <ToolbarShell testid="canvas-card-toolbar">
        <ToolbarButton
          title="Colour"
          testid="canvas-toolbar-color"
          active={paletteOpen()}
          onClick={() => setPaletteOpen((v) => !v)}
        >
          <Palette class="h-3.5 w-3.5" />
        </ToolbarButton>
        <Show when={props.canEdit}>
          <ToolbarButton title="Edit" testid="canvas-toolbar-edit" onClick={props.onEdit}>
            <Pencil class="h-3.5 w-3.5" />
          </ToolbarButton>
        </Show>
        <ToolbarButton title="Delete" testid="canvas-card-delete" onClick={props.onDelete}>
          <Trash2 class="h-3.5 w-3.5" />
        </ToolbarButton>
        <Show when={paletteOpen()}>
          <SwatchRow
            color={props.color}
            onColor={(c) => {
              props.onColor(c);
              setPaletteOpen(false);
            }}
          />
        </Show>
      </ToolbarShell>

      {/* Resize: hit area only. The cursor is the affordance. */}
      <For each={CORNERS}>
        {(c) => (
          <div
            class={`absolute h-3 w-3 ${c.class}`}
            style={{ cursor: c.cursor, 'pointer-events': 'auto' }}
            data-testid="canvas-resize-handle"
            data-corner={c.corner}
            onPointerDown={(e) => props.onResizeStart(e, c.corner)}
          />
        )}
      </For>

      {/* Connect: a dot, but only while the pointer is near that edge. */}
      <For each={SIDES}>
        {(s) => (
          <div
            class={`group absolute ${s.zone}`}
            style={{ 'pointer-events': 'auto', cursor: 'crosshair' }}
            data-testid="canvas-connect-zone"
            data-side={s.side}
            onPointerDown={(e) => props.onConnectStart(e, s.side)}
          >
            <div
              class={`absolute h-3 w-3 rounded-full border border-primary bg-surface-elevated opacity-0 transition-opacity group-hover:opacity-100 ${s.dot}`}
              data-testid="canvas-connect-dot"
            />
          </div>
        )}
      </For>
    </Show>
  );
};
