import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

/**
 * RED: Tests asserting resize handle thickness standardization (6px)
 * and EdgePanel migration from mouse to pointer events.
 */

describe('Resize Handle Standardization', () => {
  it('SplitPane uses w-1.5 (6px) for horizontal splitter, not w-2 (8px)', () => {
    const source = readFileSync(resolve(__dirname, '../SplitPane.tsx'), 'utf-8');
    // Should have w-1.5 for horizontal
    expect(source).toMatch(/w-1\.5.*cursor-col-resize/);
    // Should NOT have w-2 for horizontal
    expect(source).not.toMatch(/w-2.*cursor-col-resize/);
  });

  it('SplitPane uses h-1.5 (6px) for vertical splitter, not h-2 (8px)', () => {
    const source = readFileSync(resolve(__dirname, '../SplitPane.tsx'), 'utf-8');
    // Should have h-1.5 for vertical
    expect(source).toMatch(/h-1\.5.*cursor-row-resize/);
    // Should NOT have h-2 for vertical
    expect(source).not.toMatch(/h-2.*cursor-row-resize/);
  });

  it('EdgePanel uses w-1.5 (6px) for vertical handle, not w-1 (4px)', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Should have w-1.5 for vertical
    expect(source).toMatch(/'w-1\.5'.*isVertical\(\)/);
    // Should NOT have w-1 for vertical
    expect(source).not.toMatch(/'w-1'.*isVertical\(\)/);
  });

  it('EdgePanel uses h-1.5 (6px) for horizontal handle, not h-1 (4px)', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Should have h-1.5 for horizontal
    expect(source).toMatch(/'h-1\.5'.*!isVertical\(\)/);
    // Should NOT have h-1 for horizontal
    expect(source).not.toMatch(/'h-1'.*!isVertical\(\)/);
  });

  it('EdgePanel inline styles use min-width/min-height 6px, not 4px', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Should have 6px in inline styles
    expect(source).toMatch(/'min-width':\s*'6px'/);
    expect(source).toMatch(/'min-height':\s*'6px'/);
    // Should NOT have 4px
    expect(source).not.toMatch(/'min-width':\s*'4px'/);
    expect(source).not.toMatch(/'min-height':\s*'4px'/);
  });

  it('FloatingWindow HANDLE_DEFS uses 6px for edge handles, not 5px', () => {
    const source = readFileSync(resolve(__dirname, '../FloatingWindow.tsx'), 'utf-8');
    // Edge handles (n, s, e, w) should be 6px
    expect(source).toMatch(/edge:\s*'n'[\s\S]*height:\s*'6px'/);
    expect(source).toMatch(/edge:\s*'s'[\s\S]*height:\s*'6px'/);
    expect(source).toMatch(/edge:\s*'w'[\s\S]*width:\s*'6px'/);
    expect(source).toMatch(/edge:\s*'e'[\s\S]*width:\s*'6px'/);
    // Should NOT have 5px for edges
    expect(source).not.toMatch(/edge:\s*'[nsew]'[\s\S]*(?:height|width):\s*'5px'/);
  });

  it('FloatingWindow corner handles remain 12px', () => {
    const source = readFileSync(resolve(__dirname, '../FloatingWindow.tsx'), 'utf-8');
    // Corners should be 12x12
    expect(source).toMatch(/edge:\s*'nw'[\s\S]*width:\s*'12px'[\s\S]*height:\s*'12px'/);
    expect(source).toMatch(/edge:\s*'ne'[\s\S]*width:\s*'12px'[\s\S]*height:\s*'12px'/);
    expect(source).toMatch(/edge:\s*'sw'[\s\S]*width:\s*'12px'[\s\S]*height:\s*'12px'/);
    expect(source).toMatch(/edge:\s*'se'[\s\S]*width:\s*'12px'[\s\S]*height:\s*'12px'/);
  });
});

describe('EdgePanel Pointer Events Migration', () => {
  it('EdgePanel uses on:pointerdown, not onMouseDown', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Should have on:pointerdown
    expect(source).toMatch(/on:pointerdown/);
    // Should NOT have onMouseDown
    expect(source).not.toMatch(/onMouseDown/);
  });

  it('EdgePanel uses pointermove/pointerup events, not mousemove/mouseup', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Should have pointermove and pointerup
    expect(source).toMatch(/addEventListener\('pointermove'/);
    expect(source).toMatch(/addEventListener\('pointerup'/);
    // Should NOT have mousemove or mouseup
    expect(source).not.toMatch(/addEventListener\('mousemove'/);
    expect(source).not.toMatch(/addEventListener\('mouseup'/);
  });

  it('EdgePanel handle element has setPointerCapture call', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Should have setPointerCapture
    expect(source).toMatch(/setPointerCapture/);
  });

  it('data-testid and data-split-id attributes are preserved in SplitPane', () => {
    const source = readFileSync(resolve(__dirname, '../SplitPane.tsx'), 'utf-8');
    // Should have data-testid="resize-splitter"
    expect(source).toMatch(/data-testid="resize-splitter"/);
    // Should have data-split-id
    expect(source).toMatch(/data-split-id/);
  });
});
