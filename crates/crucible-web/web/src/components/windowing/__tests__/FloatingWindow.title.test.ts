import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';

describe('FloatingWindow title deduplication', () => {
  it('should NOT show tab title in title bar when TabBar is visible', () => {
    // Read the FloatingWindow.tsx source
    const filePath = path.join(__dirname, '../FloatingWindow.tsx');
    const source = fs.readFileSync(filePath, 'utf-8');

    // Find the title bar span (line 157-159)
    const titleBarMatch = source.match(
      /<span[^>]*class="text-xs font-medium text-zinc-300 truncate">\s*\{([^}]+)\}\s*<\/span>/
    );

    expect(titleBarMatch).toBeDefined();
    const titleExpression = titleBarMatch![1];

    // Assert that the title expression does NOT fall back to tabs()[0]?.title
    // It should only show w().title or a generic label
    expect(titleExpression).not.toMatch(/tabs\(\)\[0\]\?\.title/);
  });

  it('should show generic label or window title only', () => {
    const filePath = path.join(__dirname, '../FloatingWindow.tsx');
    const source = fs.readFileSync(filePath, 'utf-8');

    const titleBarMatch = source.match(
      /<span[^>]*class="text-xs font-medium text-zinc-300 truncate">\s*\{([^}]+)\}\s*<\/span>/
    );

    expect(titleBarMatch).toBeDefined();
    const titleExpression = titleBarMatch![1];

    // Should show either:
    // 1. w().title ?? 'Window' (window title only)
    // 2. Tab count like `${tabs().length} tab${...}`
    // 3. Some other generic label
    const isValidExpression =
      titleExpression.includes("w().title") ||
      titleExpression.includes("tabs().length");

    expect(isValidExpression).toBe(true);
  });

  it('should still have TabBar below title bar for tab titles', () => {
    const filePath = path.join(__dirname, '../FloatingWindow.tsx');
    const source = fs.readFileSync(filePath, 'utf-8');

    // Verify TabBar is still rendered (where tab titles are shown)
    expect(source).toMatch(/<TabBar\s+groupId={w\(\)\.tabGroupId}/);
  });
});
