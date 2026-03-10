import { describe, it, expect } from 'vitest';
import * as fc from 'fast-check';
import { renderMarkdown } from '../markdown';

describe('markdown property tests', () => {
  it('never outputs raw script tags regardless of input', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const withScript = `<script>alert(1)</script>${input}<script>alert(2)</script>`;
        const output = renderMarkdown(withScript);
        expect(output).not.toContain('<script>');
      }),
      { numRuns: 100 }
    );
  });

  it('never outputs unescaped event handler attributes in HTML context', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const withEventHandlers = `${input}<img onerror="alert(1)" /><div onload="alert(2)">test</div>`;
        const output = renderMarkdown(withEventHandlers);
        // The output should escape these as HTML entities, not contain raw attributes
        expect(output).not.toContain('<img onerror');
        expect(output).not.toContain('<div onload');
      }),
      { numRuns: 100 }
    );
  });

  it('never outputs unescaped javascript: protocol in attributes', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const withJsProtocol = `${input}<a href="javascript:alert(1)">click</a>`;
        const output = renderMarkdown(withJsProtocol);
        // Raw javascript: protocol in href should not appear unescaped
        expect(output).not.toContain('<a href="javascript:');
      }),
      { numRuns: 100 }
    );
  });

  it('always returns a string without throwing for any input', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const output = renderMarkdown(input);
        expect(typeof output).toBe('string');
        expect(output).toBeDefined();
      }),
      { numRuns: 100 }
    );
  });

  it('produces wikilink class for inputs containing [[...]] with valid content', () => {
    fc.assert(
      fc.property(fc.string({ minLength: 1, maxLength: 50 }).filter(s => /^[a-zA-Z0-9]+$/.test(s)), (noteName) => {
        const input = `Check [[${noteName}]] for details`;
        const output = renderMarkdown(input);
        expect(output).toContain('class="wikilink"');
      }),
      { numRuns: 100 }
    );
  });

  it('preserves wikilink data-note attribute with valid content', () => {
    fc.assert(
      fc.property(fc.string({ minLength: 1, maxLength: 50 }).filter(s => /^[a-zA-Z0-9]+$/.test(s)), (noteName) => {
        const input = `[[${noteName}]]`;
        const output = renderMarkdown(input);
        // Should contain wikilink class and data-note attribute
        expect(output).toContain('class="wikilink"');
        expect(output).toContain('data-note=');
      }),
      { numRuns: 100 }
    );
  });
});
