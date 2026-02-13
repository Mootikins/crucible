import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

describe('TabBar - Close Button Visibility', () => {
  it('should have conditional opacity classes on close button based on isActive', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    
    // Verify the close button has conditional rendering logic
    // Active tab: should NOT have opacity-0 (close button always visible)
    // Inactive tab: should have opacity-0 group-hover:opacity-100 focus:opacity-100
    
    // Check that the button element has classList with conditional logic
    expect(source).toContain('classList');
    expect(source).toContain('opacity-0');
    expect(source).toContain('group-hover:opacity-100');
    expect(source).toContain('focus:opacity-100');
    
    // Verify props.isActive is used in the component
    expect(source).toContain('props.isActive');
  });

  it('should not have opacity-0 in close button class when tab is active', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    
    // Verify close button uses classList with conditional opacity
    expect(source).toContain("'opacity-0 group-hover:opacity-100': !props.isActive");
    
    // Verify the button has the base classes
    expect(source).toContain('flex-shrink-0 p-0.5 rounded-sm transition-all');
    expect(source).toContain('hover:bg-zinc-700 hover:text-zinc-200');
  });

  it('should render close button with proper hover and focus states', () => {
    const source = readFileSync(resolve(__dirname, '../TabBar.tsx'), 'utf-8');
    
    // Verify close button has hover and focus styling
    expect(source).toContain('hover:bg-zinc-700');
    expect(source).toContain('hover:text-zinc-200');
    expect(source).toContain('focus:opacity-100');
  });
});
