import { describe, it, expect } from 'vitest';
import { placePopup, POPUP_MAX_HEIGHT, EDGE_MARGIN } from '@/lib/popup-placement';

// A composer textarea pinned near the bottom of the window — the case that
// clipped the chat autocomplete: `top-full` opened it downward into the
// overflow-hidden column below.
const bottomAnchored = { left: 40, right: 440, top: 700, bottom: 740, width: 400, height: 40 };
const topAnchored = { left: 40, right: 440, top: 20, bottom: 60, width: 400, height: 40 };
const viewport = { width: 1000, height: 768 };

describe('placePopup', () => {
  it('opens upward when the anchor is pinned to the bottom of the viewport', () => {
    const p = placePopup(bottomAnchored, viewport);
    expect(p.direction).toBe('up');
    // Anchored to the anchor's top edge, measured from the viewport bottom, so
    // the list grows away from the input instead of over it.
    expect(p.bottom).toBe(viewport.height - bottomAnchored.top);
    expect(p.top).toBeUndefined();
  });

  it('opens downward when there is room below', () => {
    const p = placePopup(topAnchored, viewport);
    expect(p.direction).toBe('down');
    expect(p.top).toBe(topAnchored.bottom);
    expect(p.bottom).toBeUndefined();
  });

  it('never exceeds the space available in the chosen direction', () => {
    // 28px above the anchor: opening up must shrink to fit, not overflow off-screen.
    const cramped = { left: 40, right: 440, top: 36, bottom: 740, width: 400, height: 704 };
    const p = placePopup(cramped, { width: 1000, height: 768 });
    expect(p.maxHeight).toBeLessThanOrEqual(p.direction === 'up' ? cramped.top : 768 - cramped.bottom);
    expect(p.maxHeight).toBeGreaterThan(0);
  });

  it('caps at the standard popup height when space is plentiful', () => {
    const p = placePopup(topAnchored, { width: 1000, height: 2000 });
    expect(p.maxHeight).toBe(POPUP_MAX_HEIGHT);
  });

  it('clamps a right-edge anchor back inside the viewport', () => {
    const offRight = { left: 880, right: 1280, top: 20, bottom: 60, width: 400, height: 40 };
    const p = placePopup(offRight, viewport);
    expect(p.left + p.width).toBeLessThanOrEqual(viewport.width - EDGE_MARGIN);
    expect(p.left).toBeGreaterThanOrEqual(0);
  });

  it('never pushes left of the viewport when the panel is wider than the window', () => {
    const anchor = { left: 4, right: 30, top: 20, bottom: 60, width: 26, height: 40 };
    const p = placePopup(anchor, { width: 200, height: 700 }, { width: 400 });
    expect(p.left).toBe(0);
  });

  it('matches the anchor width so the list lines up with the input', () => {
    expect(placePopup(topAnchored, viewport).width).toBe(topAnchored.width);
  });

  it('reports zero height when neither side has room', () => {
    // A viewport squeezed to nothing (mobile keyboard over a short window).
    // Zero is a real answer, and consumers must render it as zero rather than
    // treat it as "unset" and substitute a full-height panel — that puts the
    // popup straight back off-screen.
    const squeezed = { left: 0, right: 300, top: 4, bottom: 20, width: 300, height: 16 };
    const p = placePopup(squeezed, { width: 320, height: 24 });
    expect(p.maxHeight).toBe(0);
  });
});

// Chip popouts (model / kiln / agent pickers) are content-sized rather than
// anchor-sized, and sit next to the composer where space is tightest.
describe('placePopup for content-sized panels', () => {
  it('pulls a panel wider than its trigger back inside the right edge', () => {
    // A 24px-wide chip at x=686 opening a 221px panel in a 760px window ran
    // 147px off-screen before placement clamped it.
    const chip = { left: 686, right: 710, top: 295, bottom: 319, width: 24, height: 24 };
    const p = placePopup(chip, { width: 760, height: 700 }, { width: 221 });
    expect(p.width).toBe(221);
    // Keeps the same breathing room from the edge it keeps from the top and
    // bottom — flush against the viewport reads as clipped even when it isn't,
    // and sub-pixel widths would tip it over.
    expect(p.left + p.width).toBeLessThanOrEqual(760 - EDGE_MARGIN);
    expect(p.left).toBeGreaterThanOrEqual(0);
  });

  it('caps a long list to the space available instead of running off-screen', () => {
    const chip = { left: 20, right: 44, top: 600, bottom: 624, width: 24, height: 24 };
    // 20 kilns worth of rows asked for far more height than the viewport has.
    const p = placePopup(chip, { width: 760, height: 700 }, { width: 221, preferredHeight: 700 });
    const limit = p.direction === 'up' ? chip.top : 700 - chip.bottom;
    expect(p.maxHeight).toBeLessThanOrEqual(limit);
  });

  it('honours a gap between trigger and panel', () => {
    const p = placePopup(topAnchored, viewport, { gap: 4 });
    expect(p.top).toBe(topAnchored.bottom + 4);
  });

  it('applies the gap when flipped upward too', () => {
    const p = placePopup(bottomAnchored, viewport, { gap: 4 });
    expect(p.direction).toBe('up');
    expect(p.bottom).toBe(viewport.height - bottomAnchored.top + 4);
  });
});
