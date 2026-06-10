import { describe, it, expect } from 'vitest';
import * as icons from '../icons';

describe('windowing/icons.tsx', () => {
  it('should export all 13 icons as Lucide components', () => {
    const expectedIcons = [
      'IconPanelLeft',
      'IconPanelLeftClose',
      'IconPanelRight',
      'IconPanelRightClose',
      'IconPanelBottom',
      'IconPanelBottomClose',
      'IconSettings',
      'IconZap',
      'IconLayout',
      'IconGripVertical',
      'IconGripHorizontal',
      'IconClose',
      'IconMaximize',
      'IconMinimize',
    ];

    expectedIcons.forEach((iconName) => {
      expect(icons).toHaveProperty(iconName);
      expect(typeof icons[iconName as keyof typeof icons]).toBe('function');
    });
  });

  it('should not contain any <svg> elements (should be Lucide re-exports)', async () => {
    const fs = await import('fs');
    const path = await import('path');
    const sourceFile = path.join(__dirname, '../icons.tsx');
    const content = fs.readFileSync(sourceFile, 'utf-8');

    expect(content).not.toMatch(/<svg/);
  });
});
