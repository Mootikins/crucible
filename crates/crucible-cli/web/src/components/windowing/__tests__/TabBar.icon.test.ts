import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('TabBar - Icon Replacement', () => {
  it('should not contain ▼ text character in overflow button', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    expect(source).not.toContain('▼');
  });

  it('should render ChevronDown icon component instead of ▼', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    // Verify ChevronDown is imported
    expect(source).toContain('ChevronDown');
    // Verify it's used in the overflow button
    expect(source).toContain('<ChevronDown');
  });

  it('should have proper icon sizing (w-3 or w-3.5, h-3 or h-3.5)', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    // Find the ChevronDown usage and verify sizing
    const chevronMatch = source.match(/<ChevronDown[^>]*class="([^"]*)"[^>]*\/>/);
    expect(chevronMatch).toBeTruthy();
    const classes = chevronMatch?.[1] || '';
    expect(classes).toMatch(/w-3(?:\.5)?/);
    expect(classes).toMatch(/h-3(?:\.5)?/);
  });
});
