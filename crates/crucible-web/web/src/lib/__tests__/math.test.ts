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

  // Regression: this rule runs before markdown-it's `backticks` rule and
  // scans raw source, so a currency `$` used to close on the `$` inside a
  // later code span and eat the whole sentence as math.
  it('does not let a code span close a stray currency dollar', () => {
    const html = renderMarkdown(
      'Prices like $5 and $20 stay prose. Math is skipped in code spans — `$E = mc^2$` — really.',
    );
    expect(html).not.toContain('class="katex"');
    expect(html).toContain('$20 stay prose');
    expect(html).toContain('<code>$E = mc^2$</code>');
  });

  it('still renders real inline math in a paragraph that also has code spans', () => {
    const html = renderMarkdown('The norm $\\|v\\|_2$ is written `norm(v)` in code.');
    expect(html).toContain('class="katex"');
    expect(html).toContain('<code>norm(v)</code>');
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
