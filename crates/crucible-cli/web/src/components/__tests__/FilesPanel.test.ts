import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('FilesPanel - Icon Components', () => {
  it('FileIcon should render Lucide SVG components, not emoji', () => {
    const filePath = resolve(__dirname, '../FilesPanel.tsx');
    const content = readFileSync(filePath, 'utf-8');

    const fileIconMatch = content.match(/const FileIcon:[\s\S]*?return[\s\S]*?};/);
    
    expect(fileIconMatch).toBeDefined();
    const fileIconContent = fileIconMatch![0];
    
    expect(fileIconContent).not.toContain('📝');
    expect(fileIconContent).not.toContain('🔷');
    expect(fileIconContent).not.toContain('🟨');
    expect(fileIconContent).not.toContain('🦀');
    expect(fileIconContent).not.toContain('📋');
    expect(fileIconContent).not.toContain('⚙️');
    expect(fileIconContent).not.toContain('🎨');
    expect(fileIconContent).not.toContain('🌐');
    expect(fileIconContent).not.toContain('🌙');
    expect(fileIconContent).not.toContain('📄');

    expect(fileIconContent).toContain('FileText');
    expect(fileIconContent).toContain('FileCode');
    expect(fileIconContent).toContain('FileJson');
    expect(fileIconContent).toContain('Palette');
    expect(fileIconContent).toContain('Globe');
    expect(fileIconContent).toContain('Moon');
    expect(fileIconContent).toContain('Cog');
  });

  it('FolderIcon should render Lucide components, not emoji', () => {
    const filePath = resolve(__dirname, '../FilesPanel.tsx');
    const content = readFileSync(filePath, 'utf-8');

    const folderIconMatch = content.match(/const FolderIcon:[\s\S]*?};/);
    expect(folderIconMatch).toBeDefined();
    const folderIconContent = folderIconMatch![0];
    
    expect(folderIconContent).not.toContain('📂');
    expect(folderIconContent).not.toContain('📁');
    expect(folderIconContent).toContain('FolderOpen');
    expect(folderIconContent).toContain('Folder');
  });

  it('ChevronIcon should render Lucide component, not inline SVG', () => {
    const filePath = resolve(__dirname, '../FilesPanel.tsx');
    const content = readFileSync(filePath, 'utf-8');

    const chevronIconMatch = content.match(/const ChevronIcon:[\s\S]*?};/);
    expect(chevronIconMatch).toBeDefined();
    const chevronIconContent = chevronIconMatch![0];
    
    expect(chevronIconContent).not.toContain('<svg');
    expect(chevronIconContent).toContain('ChevronDown');
  });

  it('FilesPanel.tsx should have zero emoji characters', () => {
    const filePath = resolve(__dirname, '../FilesPanel.tsx');
    const content = readFileSync(filePath, 'utf-8');
    
    expect(content).not.toContain('📝');
    expect(content).not.toContain('🔷');
    expect(content).not.toContain('🟨');
    expect(content).not.toContain('🦀');
    expect(content).not.toContain('📋');
    expect(content).not.toContain('⚙️');
    expect(content).not.toContain('🎨');
    expect(content).not.toContain('🌐');
    expect(content).not.toContain('🌙');
    expect(content).not.toContain('📄');
    expect(content).not.toContain('📂');
    expect(content).not.toContain('📁');
  });
});
