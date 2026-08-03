/**
 * Viewport-aware placement for caret/input-anchored popups.
 *
 * The autocomplete list used to be `absolute top-full` inside the composer,
 * which put it *below* an input pinned to the bottom of ChatContent's
 * `overflow-hidden` column — so it was clipped away entirely. Placement is
 * computed against the viewport instead, and the caller renders through a
 * Portal so no ancestor's overflow or stacking context can hide it.
 */

/** Standard list height (matches the old `max-h-52` = 13rem). */
export const POPUP_MAX_HEIGHT = 208;

/** Breathing room kept between the popup and the viewport edge. */
export const EDGE_MARGIN = 8;

/** The subset of DOMRect placement needs — keeps this callable from tests. */
export interface AnchorRect {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface PopupPlacement {
  left: number;
  width: number;
  /** Set when opening downward — distance from the viewport top. */
  top?: number;
  /** Set when opening upward — distance from the viewport bottom. */
  bottom?: number;
  maxHeight: number;
  direction: 'up' | 'down';
}

export interface PlaceOptions {
  /** Height the popup wants; it is capped to what the chosen side allows. */
  preferredHeight?: number;
  /** Panel width. Defaults to the anchor's — chip popouts are content-sized. */
  width?: number;
  /** Space between anchor and panel. */
  gap?: number;
}

/**
 * Place a popup against `anchor`, preferring below but flipping above when
 * below can't fit it and above has more room.
 */
export function placePopup(
  anchor: AnchorRect,
  viewport: Viewport,
  options: PlaceOptions = {},
): PopupPlacement {
  const { preferredHeight = POPUP_MAX_HEIGHT, width = anchor.width, gap = 0 } = options;

  const spaceBelow = viewport.height - anchor.bottom - EDGE_MARGIN - gap;
  const spaceAbove = anchor.top - EDGE_MARGIN - gap;

  // Only flip when below genuinely can't fit AND above is roomier — flipping
  // on a tie would make the list jump around as the transcript grows.
  const direction: 'up' | 'down' =
    spaceBelow < preferredHeight && spaceAbove > spaceBelow ? 'up' : 'down';

  const available = direction === 'up' ? spaceAbove : spaceBelow;
  // No minimum height: forcing a floor taller than `available` would push the
  // list back off the viewport edge — the very clipping this module exists to
  // prevent. A squeezed popup scrolls internally instead.
  const maxHeight = Math.max(Math.min(preferredHeight, available), 0);

  // Clamp horizontally: a chip popout is wider than its trigger, so a trigger
  // near the right edge would otherwise push the panel off-screen. The margin
  // also absorbs sub-pixel panel widths, which flush-to-the-edge would spill.
  const left = Math.max(0, Math.min(anchor.left, viewport.width - width - EDGE_MARGIN));

  return direction === 'up'
    ? { left, width, bottom: viewport.height - anchor.top + gap, maxHeight, direction }
    : { left, width, top: anchor.bottom + gap, maxHeight, direction };
}

export interface FlyoutPlacement {
  left: number;
  top: number;
  width: number;
  maxHeight: number;
  side: 'right' | 'left';
}

/**
 * Place a submenu BESIDE its row, rather than below it.
 *
 * The sideways sibling of [`placePopup`]: a `Remote Machines ▸` flyout opens to
 * the right of the row that owns it, flipping to the left when the right has no
 * room. Chip popouts are already clamped to the viewport's right edge, so a
 * flyout hanging off one is exactly the case that flips — near the edge it is
 * the common case, not the corner one.
 *
 * Vertically it aligns with its row and then slides up only as far as it must
 * to stay on screen, so a submenu never opens above the row it came from.
 */
export function placeFlyout(
  anchor: AnchorRect,
  viewport: Viewport,
  options: { width?: number; preferredHeight?: number; gap?: number } = {},
): FlyoutPlacement {
  const { width = 220, preferredHeight = POPUP_MAX_HEIGHT, gap = 2 } = options;

  const spaceRight = viewport.width - anchor.right - gap - EDGE_MARGIN;
  // Flip only when the right genuinely cannot hold it — preferring the right
  // on a tie keeps the direction stable as the window resizes.
  const side: 'right' | 'left' = spaceRight >= width ? 'right' : 'left';
  const left =
    side === 'right'
      ? anchor.right + gap
      : Math.max(EDGE_MARGIN, anchor.left - width - gap);

  const maxHeight = Math.max(Math.min(preferredHeight, viewport.height - 2 * EDGE_MARGIN), 0);
  // Aligned with the row, then pulled back up by whatever overflows the bottom.
  const top = Math.max(EDGE_MARGIN, Math.min(anchor.top, viewport.height - maxHeight - EDGE_MARGIN));

  return { left, top, width, maxHeight, side };
}
