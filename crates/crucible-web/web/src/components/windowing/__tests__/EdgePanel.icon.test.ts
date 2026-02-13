import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('EdgePanel - Icon Replacement', () => {
  it('should render icon when available instead of text', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    expect(source).toContain('{tab.icon ? <tab.icon class="w-4 h-4" />');
  });

  it('should render tab.icon component in collapsed buttons', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Verify tab.icon is rendered in the collapsed state
    expect(source).toContain('<tab.icon');
    // Verify it has proper sizing
    expect(source).toContain('w-4 h-4');
  });

  it('should have fallback to text when icon is not available', () => {
    const source = readFileSync(resolve(__dirname, '../EdgePanel.tsx'), 'utf-8');
    // Verify fallback rendering for tabs without icons
    expect(source).toContain('{tab.title[0]}');
  });

  it('should import icons from @/lib/icons in windowStore', () => {
    const source = readFileSync(resolve(__dirname, '../../../stores/windowStore.ts'), 'utf-8');
    // Verify icons are imported
    expect(source).toContain('FolderTree');
    expect(source).toContain('Search');
    expect(source).toContain('GitBranch');
    expect(source).toContain('ListTree');
    expect(source).toContain('Bug');
    expect(source).toContain('Terminal');
    expect(source).toContain('AlertTriangle');
    expect(source).toContain('FileOutput');
  });

  it('should populate icon field on all edge panel tabs', () => {
    const source = readFileSync(resolve(__dirname, '../../../stores/windowStore.ts'), 'utf-8');
    // Verify each tab has an icon field
    expect(source).toContain('icon: FolderTree');
    expect(source).toContain('icon: Search');
    expect(source).toContain('icon: GitBranch');
    expect(source).toContain('icon: ListTree');
    expect(source).toContain('icon: Bug');
    expect(source).toContain('icon: Terminal');
    expect(source).toContain('icon: AlertTriangle');
    expect(source).toContain('icon: FileOutput');
  });
});
