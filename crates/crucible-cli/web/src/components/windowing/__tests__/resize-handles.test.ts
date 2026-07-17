import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

/**
 * Separator + panel-chrome contract (Obsidian-style):
 * - Visible separators are 1px lines (w-px/h-px), never filled bars.
 * - The pointer grab zone is widened invisibly via an after: pseudo-element.
 * - Panel toggle controls live in fixed slots that don't move, disappear, or
 *   resize when a panel collapses/expands.
 */

describe('1px separators with widened grab zones', () => {
  it('SplitPane splitter is a 1px line, not a filled bar', () => {
    const source = readFileSync(resolve(__dirname, '../SplitPane.tsx'), 'utf-8');
    expect(source).toMatch(/w-px cursor-col-resize/);
    expect(source).toMatch(/h-px cursor-row-resize/);
    expect(source).not.toMatch(/w-1\.5|w-2|h-1\.5|h-2/);
  });

  it('SplitPane splitter extends its pointer target via after: pseudo', () => {
    const source = readFileSync(resolve(__dirname, '../SplitPane.tsx'), 'utf-8');
    expect(source).toMatch(/after:absolute/);
    expect(source).toMatch(/after:-inset-x-1/);
    expect(source).toMatch(/after:-inset-y-1/);
  });

  it('EdgePanel handle is a 1px line, not a filled bar', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/w-px cursor-col-resize/);
    expect(source).toMatch(/h-px cursor-row-resize/);
    expect(source).not.toMatch(/w-1\.5|h-1\.5/);
    expect(source).not.toMatch(/'min-width':\s*'6px'|'min-height':\s*'6px'/);
  });

  it('EdgePanel handle extends its pointer target via after: pseudo', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/after:absolute/);
    expect(source).toMatch(/after:-inset-x-1/);
    expect(source).toMatch(/after:-inset-y-1/);
  });

  it('handles carry no grip glyphs (1px separators render clean)', () => {
    for (const file of ['../SplitPane.tsx', '../EdgePanel.tsx']) {
      const source = readFileSync(resolve(__dirname, file), 'utf-8');
      expect(source).not.toMatch(/IconGripVertical|IconGripHorizontal/);
    }
  });

  it('expanded EdgePanel content has no border of its own (the handle line is the separator)', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).not.toMatch(/'border-r border-hairline'/);
    expect(source).not.toMatch(/'border-t border-hairline'/);
  });

  it('collapsed strip border faces the center for each position', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/'flex-col border-r':\s*props\.position === 'left'/);
    expect(source).toMatch(/'flex-col border-l':\s*props\.position === 'right'/);
    expect(source).toMatch(/'flex-row border-t':\s*!isVertical\(\)/);
  });

  it('FloatingWindow invisible edge grab zones stay 6px, corners 12px', () => {
    const source = readFileSync(resolve(__dirname, '../FloatingWindow.tsx'), 'utf-8');
    expect(source).toMatch(/edge:\s*'n'[\s\S]*height:\s*'6px'/);
    expect(source).toMatch(/edge:\s*'w'[\s\S]*width:\s*'6px'/);
    expect(source).toMatch(/edge:\s*'nw'[\s\S]*width:\s*'12px'[\s\S]*height:\s*'12px'/);
  });
});

describe('ribbon chrome (Obsidian-style persistent edge bars)', () => {
  it('the ribbon renders unconditionally — panels grow out of an always-visible bar', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Left ribbon before the panel, right/bottom ribbons after it, all
    // OUTSIDE the collapsed-state <Show> so they never disappear.
    expect(source).toMatch(/\{props\.position === 'left' && <EdgeRibbon position="left" \/>\}/);
    expect(source).toMatch(/\{props\.position !== 'left' && <EdgeRibbon position=\{props\.position\} \/>\}/);
    // The old expand-slot button is gone: ribbon icons + the fixed top
    // toggle cover it. (mt-auto is reserved for the bottom-pinned settings
    // gear — Obsidian's own layout.)
    expect(source).not.toMatch(/edge-expand-/);
    expect(source).not.toMatch(/order-last ml-auto/);
  });

  it('ribbon icons toggle their panel: expand + activate, collapse on active click', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/setEdgePanelCollapsed\(props\.position, false\)/);
    expect(source).toMatch(/setEdgePanelCollapsed\(props\.position, true\)/);
    expect(source).not.toMatch(/openFlyout/);
  });

  it('every ribbon leads with its panel toggle; the left ribbon hosts command buttons', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Explicit expand/collapse at the top (leading, for the bottom bar).
    expect(source).toMatch(/data-testid=\{`ribbon-toggle-\$\{props\.position\}`\}/);
    // Obsidian-style command buttons: palette + new session up top,
    // settings gear pinned at the bottom.
    expect(source).toMatch(/ribbon-cmd-palette/);
    expect(source).toMatch(/ribbon-cmd-new-session/);
    expect(source).toMatch(/testId="ribbon-cmd-settings"\n          bottom/);
  });

  it('ribbon borders face the panel/center for each position', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/'flex-col border-r':\s*props\.position === 'left'/);
    expect(source).toMatch(/'flex-col border-l':\s*props\.position === 'right'/);
    expect(source).toMatch(/'flex-row border-t':\s*!isVertical\(\)/);
  });

  it('the ribbon toggle is the ONLY in-panel collapse control (no duplicate in the tab bar)', () => {
    // An expanded panel used to show the ribbon toggle AND an in-tab-bar
    // collapse button in the same corner with the same icon. The ribbon
    // toggle (always visible, never moves) is canonical.
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    expect(source).not.toMatch(/edge-collapse-/);
    expect(source).not.toMatch(/IconPanel(Left|Right|Bottom)Close/);
  });

  it('every panel toggle glyph is w-4 (Lucide bare default is a jarring 24px)', () => {
    const edgePanel = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(edgePanel).not.toMatch(/<IconPanel(Left|Right|Bottom)(Close)? \/>/);
    expect(edgePanel).toMatch(/<IconPanelLeftClose class="w-4 h-4" \/>/);
  });
});

describe('EdgePanel pointer events', () => {
  it('EdgePanel uses pointer events with capture, not mouse events', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/on:pointerdown/);
    expect(source).toMatch(/addEventListener\('pointermove'/);
    expect(source).toMatch(/addEventListener\('pointerup'/);
    expect(source).toMatch(/setPointerCapture/);
    expect(source).not.toMatch(/onMouseDown|addEventListener\('mousemove'|addEventListener\('mouseup'/);
  });

  it('data-testid and data-split-id attributes are preserved in SplitPane', () => {
    const source = readFileSync(resolve(__dirname, '../SplitPane.tsx'), 'utf-8');
    expect(source).toMatch(/data-testid="resize-splitter"/);
    expect(source).toMatch(/data-split-id/);
  });
});
