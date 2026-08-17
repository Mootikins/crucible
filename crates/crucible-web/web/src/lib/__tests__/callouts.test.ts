import { describe, expect, it } from 'vitest';
import { renderMarkdown } from '../markdown';
import { readFileSync } from 'fs';
import { resolve } from 'path';
import { CALLOUT_KINDS, CALLOUT_RGB, resolveCalloutKind } from '../callouts';

describe('callout rendering (through the full sanitized pipeline)', () => {
  it('renders a titled callout with icon, title, and body', () => {
    const html = renderMarkdown('> [!note] Custom Title\n> Body text here');
    expect(html).toContain('data-callout="note"');
    expect(html).toContain('class="callout"');
    expect(html).toContain('callout-icon');
    expect(html).toContain('<span class="callout-title-text">Custom Title</span>');
    expect(html).toContain('Body text here');
    expect(html).not.toContain('[!note]');
  });

  it('defaults the title to the capitalized typed word', () => {
    const html = renderMarkdown('> [!warning]\n> Watch out');
    expect(html).toContain('<span class="callout-title-text">Warning</span>');
  });

  it('resolves aliases to canonical kinds but titles as typed', () => {
    const html = renderMarkdown('> [!caution] \n> Careful');
    expect(html).toContain('data-callout="warning"');
    expect(html).toContain('<span class="callout-title-text">Caution</span>');
  });

  it('renders every canonical kind with its own data-callout', () => {
    for (const kind of CALLOUT_KINDS) {
      const html = renderMarkdown(`> [!${kind}]\n> body`);
      expect(html, kind).toContain(`data-callout="${kind}"`);
    }
  });

  it('unknown types fall back to note styling', () => {
    const html = renderMarkdown('> [!wat] Strange\n> body');
    expect(html).toContain('data-callout="note"');
    expect(html).toContain('Strange');
  });

  it('foldable-collapsed renders a closed <details> with <summary>', () => {
    const html = renderMarkdown('> [!tip]- Hidden depths\n> secret');
    expect(html).toMatch(/<details[^>]*class="callout"/);
    expect(html).not.toMatch(/<details[^>]*open/);
    expect(html).toContain('<summary class="callout-title">');
  });

  it('foldable-open renders <details open>', () => {
    const html = renderMarkdown('> [!tip]+ Shown\n> visible');
    expect(html).toMatch(/<details[^>]*open/);
  });

  it('title-only callout drops the empty body paragraph', () => {
    const html = renderMarkdown('> [!info] Just a banner');
    expect(html).toContain('Just a banner');
    expect(html).not.toContain('<p></p>');
  });

  it('renders markdown inside the body', () => {
    const html = renderMarkdown('> [!example]\n> Some **bold** and `code`');
    expect(html).toContain('<strong>bold</strong>');
    expect(html).toContain('code');
  });

  it('escapes HTML in the title', () => {
    const html = renderMarkdown('> [!note] <img src=x onerror=alert(1)>\n> body');
    expect(html).not.toContain('<img');
  });

  it('leaves plain blockquotes untouched', () => {
    const html = renderMarkdown('> just a quote');
    expect(html).toContain('<blockquote>');
    expect(html).not.toContain('callout');
  });

  it('multi-paragraph callouts keep all paragraphs inside', () => {
    const html = renderMarkdown('> [!abstract] Sum\n> first\n>\n> second');
    const open = html.indexOf('data-callout="abstract"');
    const close = html.lastIndexOf('</div>');
    expect(open).toBeGreaterThan(-1);
    expect(html.slice(open, close)).toContain('first');
    expect(html.slice(open, close)).toContain('second');
  });
});

describe('resolveCalloutKind', () => {
  it('is case-insensitive and alias-aware', () => {
    expect(resolveCalloutKind('NOTE')).toBe('note');
    expect(resolveCalloutKind('TLDR')).toBe('abstract');
    expect(resolveCalloutKind('error')).toBe('danger');
    expect(resolveCalloutKind('nonsense')).toBe('note');
  });
});

describe('CALLOUT_RGB mirrors index.css', () => {
  // index.css owns the rendered colour via `--callout-rgb`; CALLOUT_RGB is the
  // same table in TS, for callers that need the accent as data rather than as
  // a cascading custom property. The CSS comment says "keep in sync" and
  // nothing enforced it — this does. Without it the duplicate silently drifts,
  // and a drifted mirror is worse than no mirror.
  const css = readFileSync(resolve(__dirname, '../../index.css'), 'utf-8');

  /** The `--callout-rgb` a kind actually resolves to: its own override if it
   * declares one, otherwise the base `.callout` value it inherits. */
  const cssRgbFor = (kind: string): string => {
    const override = new RegExp(
      `\\.callout\\[data-callout='${kind}'\\][^{]*\\{([^}]*)\\}`,
      's',
    ).exec(css);
    const own = override && /--callout-rgb:\s*([^;]+);/.exec(override[1]);
    if (own) return own[1].trim();
    return /\.callout \{[^}]*?--callout-rgb:\s*([^;]+);/s.exec(css)![1].trim();
  };

  it.each(CALLOUT_KINDS)('%s has the same accent in both files', (kind) => {
    expect(CALLOUT_RGB[kind]).toBe(cssRgbFor(kind));
  });

  it('covers every kind, so a new callout cannot skip the table', () => {
    expect(Object.keys(CALLOUT_RGB).sort()).toEqual([...CALLOUT_KINDS].sort());
  });
});
