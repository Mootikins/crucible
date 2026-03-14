import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

/** Read the combined source of all SessionPanel-related files. */
function readSessionPanelSources(): string {
  const files = [
    '../SessionPanel.tsx',
    '../ProjectSection.tsx',
    '../SessionSection.tsx',
    '../SessionFooter.tsx',
  ];
  return files
    .map((f) => readFileSync(resolve(__dirname, f), 'utf-8'))
    .join('\n');
}

describe('SessionPanel - Icon Replacement', () => {
  it('should not contain ↻ text character in refresh button', () => {
    const source = readSessionPanelSources();
    expect(source).not.toContain('↻');
  });

  it('should render RefreshCw icon component instead of ↻', () => {
    const source = readSessionPanelSources();
    // Verify RefreshCw is imported
    expect(source).toContain('RefreshCw');
    // Verify it's used in the refresh button
    expect(source).toContain('<RefreshCw');
  });

  it('should not contain + prefix text in add buttons', () => {
    const source = readSessionPanelSources();
    // Should not have "+ Add Project" or "+ New Session" as text
    expect(source).not.toContain('+ Add Project');
    expect(source).not.toContain('+ New Session');
  });

  it('should render Plus icon component in add buttons', () => {
    const source = readSessionPanelSources();
    // Verify Plus is imported
    expect(source).toContain('Plus');
    // Verify it's used (should appear at least twice for two add buttons)
    const plusMatches = source.match(/<Plus/g);
    expect(plusMatches?.length).toBeGreaterThanOrEqual(2);
  });

  it('should have proper icon sizing for Plus icon', () => {
    const source = readSessionPanelSources();
    // Find Plus icon usages and verify sizing
    const plusMatches = source.match(/<Plus[^>]*class="([^"]*)"[^>]*\/>/g);
    expect(plusMatches).toBeTruthy();
    plusMatches?.forEach((match) => {
      expect(match).toMatch(/w-3(?:\.5)?/);
      expect(match).toMatch(/h-3(?:\.5)?/);
    });
  });
});
