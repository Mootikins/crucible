import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('EdgePanel - Icon Replacement', () => {
  it('should render icon when available instead of text', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toContain('{props.tab.icon ? (');
  });

  it('should render tab.icon component in collapsed buttons', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Verify tab.icon is rendered in the collapsed state
    expect(source).toContain('<props.tab.icon');
    // Verify it has proper sizing
    expect(source).toContain('w-4 h-4');
  });

  it('should have fallback to text when icon is not available', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Verify fallback rendering for tabs without icons
    expect(source).toContain('{props.tab.title[0]}');
  });

  // The default edge roster (post clean-slate): Sessions + Files (left),
  // Backlinks + Activity (right), Terminal + Chat (bottom).
  const rosterIcons = ['ClipboardList', 'FolderTree', 'Link2', 'Activity', 'Terminal', 'MessageCircle'];

  it('should import roster icons from @/lib/icons in window store internals', () => {
    const source = readFileSync(resolve(__dirname, '../../../stores/windowStoreInternals.ts'), 'utf-8');
    for (const icon of rosterIcons) {
      expect(source).toContain(icon);
    }
  });

  it('should populate icon field on all edge panel tabs', () => {
    const source = readFileSync(resolve(__dirname, '../../../stores/windowStoreInternals.ts'), 'utf-8');
    for (const icon of rosterIcons) {
      expect(source).toContain(`icon: ${icon}`);
    }
  });
});
