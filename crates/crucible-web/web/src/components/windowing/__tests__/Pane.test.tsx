import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('Pane - Empty State', () => {
  it('should render an SVG icon in the empty pane placeholder', () => {
    const paneFile = readFileSync(resolve(__dirname, '../Pane.tsx'), 'utf-8');
    
    expect(paneFile).toContain('AppWindow');
    expect(paneFile).toContain('Drop tabs here');
    expect(paneFile).toContain('flex-col');
    expect(paneFile).toContain('gap-3');
  });
});
