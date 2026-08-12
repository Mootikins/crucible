import { describe, it, expect } from 'vitest';
import {
  extractFrontmatterBlock,
  parseFrontmatterEntries,
  renderFrontmatterCardHtml,
} from '@/lib/frontmatter';

describe('extractFrontmatterBlock', () => {
  it('extracts YAML (---) frontmatter and the body offset', () => {
    const content = '---\ntitle: Hello\ntags:\n  - a\n  - b\n---\nBody text\n';
    const block = extractFrontmatterBlock(content)!;
    expect(block.format).toBe('yaml');
    expect(content.slice(block.bodyStart)).toBe('Body text\n');
    expect(block.entries).toEqual([
      { key: 'title', value: 'Hello' },
      { key: 'tags', value: ['a', 'b'] },
    ]);
  });

  it('extracts TOML (+++) frontmatter', () => {
    const content = '+++\ntitle = "Hello"\ncount = 3\ntags = ["a", "b"]\n+++\nBody\n';
    const block = extractFrontmatterBlock(content)!;
    expect(block.format).toBe('toml');
    expect(content.slice(block.bodyStart)).toBe('Body\n');
    expect(block.entries).toEqual([
      { key: 'title', value: 'Hello' },
      { key: 'count', value: '3' },
      { key: 'tags', value: ['a', 'b'] },
    ]);
  });

  it('returns null when there is no frontmatter or no closing delimiter', () => {
    expect(extractFrontmatterBlock('# Just a doc\n')).toBeNull();
    expect(extractFrontmatterBlock('---\ntitle: x\nno closer')).toBeNull();
    // A thematic break mid-document must not count: the opener has to be
    // the very first line.
    expect(extractFrontmatterBlock('text\n---\nmore\n---\n')).toBeNull();
  });

  it('does not treat --- with trailing text as an opener', () => {
    expect(extractFrontmatterBlock('--- not frontmatter\nx\n---\n')).toBeNull();
  });
});

describe('parseFrontmatterEntries', () => {
  it('parses quoted strings, bools, and inline arrays (yaml)', () => {
    const entries = parseFrontmatterEntries(
      'title: "Quoted"\ndraft: true\nkinds: [x, y]',
      'yaml',
    )!;
    expect(entries).toEqual([
      { key: 'title', value: 'Quoted' },
      { key: 'draft', value: 'true' },
      { key: 'kinds', value: ['x', 'y'] },
    ]);
  });

  it('skips blank lines and comments', () => {
    const entries = parseFrontmatterEntries('# note\n\ntitle: x', 'yaml')!;
    expect(entries).toEqual([{ key: 'title', value: 'x' }]);
  });

  it('bails to null on nested structures (fallback to raw source)', () => {
    expect(parseFrontmatterEntries('meta:\n  nested: true', 'yaml')).toBeNull();
    expect(parseFrontmatterEntries('[table]\nx = 1', 'toml')).toBeNull();
    expect(parseFrontmatterEntries('point = { x = 1 }', 'toml')).toBeNull();
  });
});

describe('renderFrontmatterCardHtml', () => {
  it('renders rows with pills for arrays and escapes HTML', () => {
    const html = renderFrontmatterCardHtml([
      { key: 'title', value: '<b>x</b>' },
      { key: 'tags', value: ['a&b'] },
    ]);
    expect(html).toContain('data-testid="fm-card"');
    expect(html).toContain('&lt;b&gt;x&lt;/b&gt;');
    expect(html).toContain('<span class="fm-pill">a&amp;b</span>');
    expect(html).not.toContain('<b>');
  });

  /**
   * Properties are reference material, not the note. On a canvas card a
   * four-key block can outweigh the prose it belongs to.
   */
  it('is collapsed by default', () => {
    const html = renderFrontmatterCardHtml([{ key: 'title', value: 'x' }]);
    expect(html).toContain('<details');
    // `open` is what a <details> renders expanded; its absence IS the collapse.
    expect(html).not.toContain('open');
  });

  it('summarises how many properties are hidden, pluralised', () => {
    expect(renderFrontmatterCardHtml([{ key: 'a', value: '1' }])).toContain('1 property');
    expect(
      renderFrontmatterCardHtml([
        { key: 'a', value: '1' },
        { key: 'b', value: '2' },
      ]),
    ).toContain('2 properties');
  });

  /**
   * The summary is a visible one-line row now, not a shrink-wrapped square
   * floated into the corner where the editor's mode toggles live. So the count
   * has to be rendered text — as a `title`/`aria-label` pair it was invisible,
   * and keeping those alongside visible text would duplicate the accessible
   * name and add a redundant tooltip.
   */
  it('shows the count as text rather than hiding it in a tooltip', () => {
    const html = renderFrontmatterCardHtml([
      { key: 'a', value: '1' },
      { key: 'b', value: '2' },
    ]);
    expect(html).toContain('<span class="fm-count">2 properties</span>');
    expect(html).not.toContain('title=');
    expect(html).not.toContain('aria-label=');
  });

  /** Escaped through the same path as the rows — the label is built, not user text, but the rows beside it are not. */
  it('keeps the rows available inside the collapsed card', () => {
    const html = renderFrontmatterCardHtml([{ key: 'tags', value: ['kiln'] }]);
    expect(html).toContain('fm-rows');
    expect(html).toContain('<span class="fm-pill">kiln</span>');
  });
});
