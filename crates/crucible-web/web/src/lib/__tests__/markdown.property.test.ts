import { describe, it, expect } from 'vitest';
import * as fc from 'fast-check';
import { renderMarkdown } from '../markdown';

describe('markdown property tests', () => {
  it('never outputs raw script tags regardless of input', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const withScript = `<script>alert(1)</script>${input}<script>alert(2)</script>`;
        const output = renderMarkdown(withScript);
        // Parse rather than substring-match, for the reason spelled out on the
        // event-handler case below.
        const doc = new DOMParser().parseFromString(output, 'text/html');
        expect(doc.querySelectorAll('script')).toHaveLength(0);
      }),
      { numRuns: 100 }
    );
  });

  it('never outputs unescaped event handler attributes in HTML context', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const withEventHandlers = `${input}<img onerror="alert(1)" /><div onload="alert(2)">test</div>`;
        const output = renderMarkdown(withEventHandlers);
        // Assert on the parsed DOM, not the serialised string. A substring
        // check reports the markup a browser would *build*, which is the
        // security question, only when nothing inert happens to contain the
        // same characters — and things legitimately do. Unparseable math puts
        // KaTeX's ParseError (which quotes the source) into a `title`
        // attribute, so `$$#<img onerror=…` leaves that exact substring in an
        // attribute value where it can never become an element. `fc.string()`
        // produces a `$$` now and then, which is why the substring form failed
        // roughly one run in a few hundred and looked like a flake.
        const doc = new DOMParser().parseFromString(output, 'text/html');
        expect(doc.querySelectorAll('img')).toHaveLength(0);
        expect(doc.querySelectorAll('[onerror], [onload]')).toHaveLength(0);
        // Any handler attribute at all, whatever the element or casing.
        const withHandlers = Array.from(doc.querySelectorAll('*')).filter((el) =>
          Array.from(el.attributes).some((a) => a.name.toLowerCase().startsWith('on')),
        );
        expect(withHandlers).toEqual([]);
      }),
      { numRuns: 100 }
    );
  });

  /**
   * The counterexample that used to fail this file intermittently (fast-check
   * seed -341303685, shrunk to `"$$#"`), pinned deterministically so the
   * property tests can never quietly stop covering it.
   */
  it('treats an event handler swallowed into a KaTeX error as inert', () => {
    const output = renderMarkdown('$$#<img onerror="alert(1)" /><div onload="alert(2)">test</div>');
    const doc = new DOMParser().parseFromString(output, 'text/html');
    expect(doc.querySelectorAll('img, div, [onerror], [onload]')).toHaveLength(0);

    // The raw substring IS present, in the katex-error title, and that is not
    // a defect: escaping it at the source would not survive DOMPurify's
    // parse/re-serialise (HTML never escapes `<` in an attribute value). This
    // asserts the distinction the old substring check got wrong.
    expect(output).toContain('<img onerror');
    expect(doc.querySelector('.katex-error')?.getAttribute('title')).toContain('<img onerror');
  });

  it('never outputs unescaped javascript: protocol in attributes', () => {
    fc.assert(
      fc.property(fc.string(), (input) => {
        const withJsProtocol = `${input}<a href="javascript:alert(1)">click</a>`;
        const output = renderMarkdown(withJsProtocol);
        // Parse the rendered HTML and inspect every anchor's real href. This
        // catches quoting/casing/entity variants (e.g. ` JavaScript:` or
        // `&#106;avascript:`) that a single exact-substring check would miss —
        // DOMParser decodes the attribute the way a browser would before we
        // test the scheme.
        const doc = new DOMParser().parseFromString(output, 'text/html');
        const dangerousAnchors = Array.from(doc.querySelectorAll('a[href]')).filter((a) => {
          const href = (a.getAttribute('href') ?? '').trim().toLowerCase();
          return href.startsWith('javascript:');
        });
        expect(dangerousAnchors).toEqual([]);
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
