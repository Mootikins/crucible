/**
 * Rendered prose gives a link ONE treatment in both themes (R6).
 *
 * A wikilink is ember text with no underline at rest, and the underline is
 * the follow affordance on hover. An external link is underlined always,
 * because it leaves the app. A heading is never underlined.
 *
 * The light theme broke all three inside the editor — see
 * `editor/__tests__/editor-theme.test.ts` and the wikilink gate beside it.
 * This file covers the OTHER surface: chat turns, note previews and hover
 * cards, which render markdown to HTML rather than to CodeMirror.
 *
 * jsdom runs the cascade, so the at-rest decoration is READ off the elements.
 * `:hover` never matches in jsdom, so the hover rule is read out of the
 * stylesheet through the CSSOM instead — parsed, never grepped.
 */
import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

let sheet: CSSStyleSheet;

beforeAll(() => {
  // What these rules have to beat. Tailwind's typography plugin emits its
  // link treatment at `:where()` weight under the `prose` class, and
  // PROSE_CLASS (lib/markdown.ts) turns the underline OFF there for EVERY
  // anchor — which is why an external link needs a rule of its own. Without
  // this stand-in the browser default underlines `a`, and the external-link
  // gate would pass with no rule at all.
  const typography = document.createElement('style');
  typography.textContent = '.prose :where(a) { text-decoration: none; }';
  document.head.appendChild(typography);

  const style = document.createElement('style');
  style.textContent = readFileSync(
    resolve(process.cwd(), 'src/styles/refine-states.css'),
    'utf8',
  );
  document.head.appendChild(style);
  sheet = style.sheet!;
});

afterEach(() => {
  document.body.innerHTML = '';
});

function prose(html: string): void {
  document.body.innerHTML = `<div class="prose">${html}</div>`;
}

const decorationOf = (id: string) =>
  getComputedStyle(document.getElementById(id)!).textDecoration;

/** Style rules whose selector names `.wikilink` in a hover state. */
const hoverRules = () =>
  [...sheet.cssRules]
    .filter((r): r is CSSStyleRule => r instanceof CSSStyleRule)
    .filter((r) => r.selectorText.includes('.wikilink') && r.selectorText.includes(':hover'));

describe('link treatment in rendered prose', () => {
  it('leaves a wikilink unlined at rest, in prose and outside it', () => {
    prose('<a id="w" class="wikilink" href="#" data-note="Note">Note</a>');
    expect(decorationOf('w')).toBe('none');

    // Hover cards and chat turns render a wikilink outside `.prose`, where
    // the browser default underlines an anchor and no typography rule runs.
    document.body.innerHTML =
      '<div><a id="bare" class="wikilink" href="#" data-note="Note">Note</a></div>';
    expect(decorationOf('bare')).toBe('none');
  });

  it('paints the wikilink pill from ONE rule set, in this file', () => {
    prose('<a id="w" class="wikilink" href="#" data-note="Note">Note</a>');
    const style = getComputedStyle(document.getElementById('w')!);
    // jsdom runs the cascade but does not substitute `var()`, so what comes
    // back is the token the cascade selected. Following the name into the
    // token layer is `lib/__tests__/contrast.test.ts`'s job, not this one.
    expect(style.color).toContain('--color-primary');
    expect(style.backgroundColor).toContain('--color-primary');

    // The chat transcript and the note reading view render the SAME class, so
    // they can only disagree if the class is written twice. It was: the pill
    // sat in index.css and the decoration here, and the two halves drifted.
    const indexCss = readFileSync(resolve(process.cwd(), 'src/index.css'), 'utf8');
    expect(indexCss).not.toMatch(/^\s*\.wikilink\b/m);
  });

  it('underlines a wikilink on hover, spans included', () => {
    const rules = hoverRules().filter((r) => r.style.textDecoration === 'underline');
    expect(rules.length).toBeGreaterThan(0);
    // The markdown renderer can nest a span inside the anchor, and a
    // descendant's own decoration would otherwise win there.
    expect(rules.some((r) => r.selectorText.includes('.wikilink:hover span'))).toBe(true);
  });

  it('underlines an external link always', () => {
    prose('<a id="x" href="https://example.com">Example</a>');
    expect(decorationOf('x')).toBe('underline');
  });

  it('never underlines a heading, nor a link that is a whole heading', () => {
    prose('<h2 id="h">Heading</h2><h3 id="h3"><a id="hl" href="#">Linked heading</a></h3>');
    expect(decorationOf('h')).toBe('none');
    // The external-link rule must not reach into a heading.
    expect(decorationOf('hl')).toBe('none');
  });

  it('states every rule once, for both themes', () => {
    // A rule inside a theme query is a rule the other theme does not get,
    // and "identical in both themes" is the whole requirement here.
    const themed = [...sheet.cssRules].filter(
      (r) => r instanceof CSSMediaRule || r.cssText.includes('[data-theme'),
    );
    expect(themed).toEqual([]);
  });
});
