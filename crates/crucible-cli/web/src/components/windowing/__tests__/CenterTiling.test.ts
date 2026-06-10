import { describe, it, expect } from 'vitest';
import fs from 'fs';
import path from 'path';

describe('CenterTiling', () => {
  it('should not contain dev-only ratio buttons in source', () => {
    const filePath = path.join(__dirname, '../CenterTiling.tsx');
    const content = fs.readFileSync(filePath, 'utf-8');
    
    // Assert that "Set ratio" text does not exist in the component source
    expect(content).not.toContain('Set ratio');
  });
});
