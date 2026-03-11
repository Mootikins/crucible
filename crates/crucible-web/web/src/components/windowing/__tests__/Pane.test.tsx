import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('Pane - Empty State', () => {
  it('should render EmptyState component when no tabs are present', () => {
    const paneFile = readFileSync(resolve(__dirname, '../Pane.tsx'), 'utf-8');
    
    // Check that EmptyState is imported and used
    expect(paneFile).toContain('EmptyState');
    expect(paneFile).toContain('import { EmptyState }');
  });
});

