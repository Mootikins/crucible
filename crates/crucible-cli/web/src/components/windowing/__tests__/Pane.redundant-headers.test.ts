import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('Pane.tsx - Redundant Headers Removal', () => {
  const paneSource = readFileSync(
    resolve(__dirname, '../Pane.tsx'),
    'utf-8'
  );

  it('file case should NOT contain header div with tab.title', () => {
    // The file case should not have a div with "flex items-center gap-2 mb-4 pb-3 border-b"
    // that contains {tab.title}
    const fileCase = paneSource.match(/case 'file':([\s\S]*?)case 'document'/)?.[1] || '';
    expect(fileCase).not.toMatch(/flex items-center gap-2 mb-4 pb-3 border-b/);
    expect(fileCase).not.toMatch(/<span class="text-sm text-zinc-300">\{tab\.title\}<\/span>/);
  });

  it('preview case should NOT contain paragraph with tab.title', () => {
    // The preview case should not have a <p> tag with {tab.title}
    const previewCase = paneSource.match(/case 'preview':([\s\S]*?)case 'terminal'/)?.[1] || '';
    expect(previewCase).not.toMatch(/<p class="text-sm text-zinc-400 mt-3 text-center">\{tab\.title\}<\/p>/);
  });

  it('tool case should NOT contain "Tool: {tab.title}" div', () => {
    // The tool case should not have a div with "Tool: {tab.title}"
    const toolCase = paneSource.match(/case 'tool':([\s\S]*?)default:/)?.[1] || '';
    expect(toolCase).not.toMatch(/<div class="text-zinc-300 text-sm">Tool: \{tab\.title\}<\/div>/);
  });

  it('document case should still have "Document Preview" heading (not a tab title dupe)', () => {
    // Document case should keep its "Document Preview" heading (it's a type label, not tab title)
    const documentCase = paneSource.match(/case 'document':([\s\S]*?)case 'preview'/)?.[1] || '';
    expect(documentCase).toMatch(/Document Preview/);
  });

  it('TabBar should still show tab titles in expanded view', () => {
    // Verify TabBar.tsx still renders tab.title (source of truth for titles)
    const tabBarSource = readFileSync(
      resolve(__dirname, '../TabBar.tsx'),
      'utf-8'
    );
    expect(tabBarSource).toMatch(/\{props\.tab\.title\}/);
  });
});
