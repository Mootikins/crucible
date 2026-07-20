import { describe, expect, it } from 'vitest';
import { renderMarkdown } from '../markdown';
import { CALLOUT_KINDS, resolveCalloutKind } from '../callouts';

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
