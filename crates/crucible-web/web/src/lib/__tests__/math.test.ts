import { describe, it, expect } from 'vitest';
import { renderMarkdown } from '../markdown';
import { renderMath } from '../math';

describe('KaTeX math rendering', () => {
  it('renders inline $…$ as KaTeX HTML', () => {
    const html = renderMarkdown('Euler: $e^{i\\pi}+1=0$ done.');
    expect(html).toContain('class="katex"');
    // The literal `$` delimiters are consumed, not shown.
    expect(html).not.toContain('$e^');
    expect(html).toContain('done.');
  });

  it('renders $$…$$ as display math', () => {
    const html = renderMarkdown('$$\\int_0^1 x^2\\,dx$$');
    expect(html).toContain('katex');
    // displayMode wraps in katex-display.
    expect(html).toContain('katex-display');
  });

  it('survives DOMPurify (spans/classes/styles kept)', () => {
    const html = renderMarkdown('$x^2$');
    // KaTeX uses inline styles for struts — these must not be stripped.
    expect(html).toContain('class="katex"');
    expect(html).toMatch(/style="[^"]*"/);
  });

  it('does NOT treat currency as math', () => {
    const html = renderMarkdown('It costs $5 and $10 total.');
    expect(html).not.toContain('class="katex"');
    expect(html).toContain('$5');
    expect(html).toContain('$10');
  });

  it('leaves an escaped \\$ as a literal dollar', () => {
    const html = renderMarkdown('Price is \\$42 flat.');
    expect(html).not.toContain('class="katex"');
    expect(html).toContain('$42');
  });

  it('renders malformed math in place instead of throwing', () => {
    expect(() => renderMath('\\frac{', false)).not.toThrow();
    const html = renderMarkdown('$\\frac{$');
    // Errored math still produces a katex node (red error text), no crash.
    expect(typeof html).toBe('string');
  });
});
