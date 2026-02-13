import { describe, it, expect } from 'vitest';
import * as icons from '../icons';

describe('Icon Module', () => {
  it('should export all required icons', () => {
    const requiredIcons = [
      'PanelLeft',
      'PanelLeftClose',
      'PanelRight',
      'PanelRightClose',
      'PanelBottom',
      'PanelBottomClose',
      'X',
      'Maximize2',
      'Minimize2',
      'GripVertical',
      'GripHorizontal',
      'Settings',
      'Zap',
      'LayoutDashboard',
      'ChevronDown',
      'RefreshCw',
      'Plus',
      'FileText',
      'FileCode',
      'File',
      'Folder',
      'FolderOpen',
      'FileJson',
      'Palette',
      'Globe',
      'Moon',
      'Cog',
      'FolderTree',
      'Search',
      'GitBranch',
      'ListTree',
      'Bug',
      'Terminal',
      'AlertTriangle',
      'FileOutput',
      'AppWindow',
    ];

    requiredIcons.forEach((iconName) => {
      expect(icons[iconName as keyof typeof icons]).toBeDefined();
      expect(typeof icons[iconName as keyof typeof icons]).toBe('function');
    });
  });
});
