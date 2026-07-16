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
    expect(source).not.toMatch(/'border-r border-zinc-800'/);
    expect(source).not.toMatch(/'border-t border-zinc-800'/);
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

describe('stable panel toggle placement', () => {
  it('collapsed strip anchors the expand button in a top h-9 slot on vertical panels', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/'w-10 h-9 border-b border-zinc-800':\s*isVertical\(\)/);
    // Old design pinned it to the bottom with mt-auto — must not come back.
    expect(source).not.toMatch(/mt-auto/);
  });

  it('bottom strip pins the expand button to the right end so it does not drift with tab count', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toMatch(/'order-last ml-auto h-9 px-2':\s*!isVertical\(\)/);
  });

  it('expanded edge tab bars keep an in-place collapse button', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    expect(source).toMatch(/data-testid=\{`edge-collapse-\$\{props\.position\}`\}/);
    expect(source).toMatch(/IconPanelLeftClose|IconPanelRightClose|IconPanelBottomClose/);
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
