import { describe, it, expect } from 'vitest';
import {
  extractFrontmatterBlock,
  parseFrontmatterEntries,
  renderFrontmatterCard,
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

  it('bails to null on structures it cannot represent (fallback to a raw card)', () => {
    expect(parseFrontmatterEntries('[table]\nx = 1', 'toml')).toBeNull();
    expect(parseFrontmatterEntries('point = { x = 1 }', 'toml')).toBeNull();
    // A bare key with nothing under it names no value.
    expect(parseFrontmatterEntries('meta:\ntitle: x', 'yaml')).toBeNull();
  });

  /**
   * A nested map and a block scalar are the two shapes REAL notes use that the
   * first flat parser rejected — every agent card carries a `tools:` map, and
   * a long `description:` is written as a folded scalar. Both surfaces fell
   * back, and their fallbacks did not agree.
   */
  it('flattens a nested map to dotted keys (yaml)', () => {
    const entries = parseFrontmatterEntries(
      'type: agent-card\ntools:\n  semantic_search: true\n  create_note: ask\n',
      'yaml',
    )!;
    expect(entries).toEqual([
      { key: 'type', value: 'agent-card' },
      { key: 'tools.semantic_search', value: 'true' },
      { key: 'tools.create_note', value: 'ask' },
    ]);
  });

  it('flattens a map nested more than one level deep', () => {
    const entries = parseFrontmatterEntries('a:\n  b:\n    c: 1\n', 'yaml')!;
    expect(entries).toEqual([{ key: 'a.b.c', value: '1' }]);
  });

  it('keeps a dash list under a nested key', () => {
    const entries = parseFrontmatterEntries('meta:\n  tags:\n    - one\n    - two\n', 'yaml')!;
    expect(entries).toEqual([{ key: 'meta.tags', value: ['one', 'two'] }]);
  });

  it('folds a `>-` block scalar into one line', () => {
    const entries = parseFrontmatterEntries(
      'description: >-\n  one line\n  and the next\nstatus: done\n',
      'yaml',
    )!;
    expect(entries).toEqual([
      { key: 'description', value: 'one line and the next' },
      { key: 'status', value: 'done' },
    ]);
  });

  it('breaks a folded scalar at a blank line', () => {
    const entries = parseFrontmatterEntries('d: >\n  one\n\n  two\n', 'yaml')!;
    expect(entries).toEqual([{ key: 'd', value: 'one\ntwo' }]);
  });

  it('keeps every line break of a `|` block scalar', () => {
    const entries = parseFrontmatterEntries('d: |\n  one\n  two\n', 'yaml')!;
    expect(entries).toEqual([{ key: 'd', value: 'one\ntwo' }]);
  });

  it('reads a block scalar that keeps its own deeper indent', () => {
    const entries = parseFrontmatterEntries('d: |\n  one\n    two\n', 'yaml')!;
    expect(entries).toEqual([{ key: 'd', value: 'one\n  two' }]);
  });

  it('bails on a block-scalar header with no block under it', () => {
    expect(parseFrontmatterEntries('d: |\ntitle: x', 'yaml')).toBeNull();
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

  /**
   * The collapsed row is a control: a hairline box at row height across the
   * content column. Its rules in `styles/refine-touch.css` select
   * `.fm-card > .fm-summary`, so the summary must stay the card's DIRECT
   * child. Wrapping it would drop the border and the tap height silently.
   */
  it('keeps the summary as the direct child of the card', () => {
    const host = document.createElement('div');
    host.innerHTML = renderFrontmatterCardHtml([
      { key: 'a', value: '1' },
      { key: 'b', value: '2' },
    ]);
    const card = host.querySelector('[data-testid="fm-card"]')!;
    const summary = card.querySelector('[data-testid="fm-summary"]')!;
    expect(summary.parentElement).toBe(card);
    expect(summary.tagName).toBe('SUMMARY');
    // The caret and the count are the whole row, and the row is the target.
    expect(summary.querySelector('.fm-caret')).not.toBeNull();
    expect(summary.textContent).toBe('2 properties');
  });

  /** Escaped through the same path as the rows — the label is built, not user text, but the rows beside it are not. */
  it('keeps the rows available inside the collapsed card', () => {
    const html = renderFrontmatterCardHtml([{ key: 'tags', value: ['kiln'] }]);
    expect(html).toContain('fm-rows');
    expect(html).toContain('<span class="fm-pill">kiln</span>');
  });
});

describe('properties: expanded', () => {
  it('opens the card when a note asks for it', () => {
    const html = renderFrontmatterCardHtml([
      { key: 'title', value: 'Foo' },
      { key: 'properties', value: 'expanded' },
    ]);
    expect(html).toContain('<details class="fm-card" data-testid="fm-card" open>');
  });

  it('leaves the card closed by default', () => {
    const html = renderFrontmatterCardHtml([{ key: 'title', value: 'Foo' }]);
    expect(html).not.toContain(' open>');
  });

  it('accepts `collapsed` as an explicit statement of the default', () => {
    const html = renderFrontmatterCardHtml([{ key: 'properties', value: 'collapsed' }]);
    expect(html).not.toContain(' open>');
  });

  it('ignores a value it does not recognise rather than guessing', () => {
    const html = renderFrontmatterCardHtml([{ key: 'properties', value: 'yes please' }]);
    expect(html).not.toContain(' open>');
  });

  it('reads the key case-insensitively and tolerates the array form', () => {
    expect(renderFrontmatterCardHtml([{ key: 'Properties', value: 'Expanded' }])).toContain(
      ' open>',
    );
    expect(renderFrontmatterCardHtml([{ key: 'properties', value: ['expanded'] }])).toContain(
      ' open>',
    );
  });

  // Hiding it would leave a card open for a reason invisible in the note.
  it('still renders the key as an ordinary row', () => {
    const html = renderFrontmatterCardHtml([{ key: 'properties', value: 'expanded' }]);
    expect(html).toContain('properties');
    expect(html).toContain('expanded');
  });
});

/**
 * The ONE card both surfaces render. The reading view used to drop a block the
 * flat parser rejected, while live preview kept its raw source on screen — the
 * same note showed a mono YAML dump in the editor and nothing at all in the
 * reading view. Every fallback lives here now, so neither caller can invent
 * its own.
 */
describe('renderFrontmatterCard', () => {
  const block = (content: string) => extractFrontmatterBlock(content)!;

  it('renders the rows card for a block the parser understands', () => {
    const html = renderFrontmatterCard(block('---\ntitle: x\n---\nbody\n'))!;
    expect(html).toContain('data-testid="fm-card"');
    expect(html).toContain('1 property');
    expect(html).not.toContain('fm-raw');
  });

  it('renders a raw card, not nothing, for a block the parser rejects', () => {
    const card = block('---\npoint = { x = 1 }\n---\nbody\n');
    // TOML inline table: beyond the flat parser on purpose.
    expect(card.entries).toBeNull();
    const html = renderFrontmatterCard(card)!;
    expect(html).toContain('data-testid="fm-card"');
    expect(html).toContain('data-testid="fm-raw"');
    expect(html).toContain('point = { x = 1 }');
    // The source is escaped, never markup.
    expect(html).not.toContain('<x');
  });

  it('escapes the raw source', () => {
    const card = block('---\nx = { a = "<script>" }\n---\n');
    const html = renderFrontmatterCard(card)!;
    expect(html).toContain('&lt;script&gt;');
    expect(html).not.toContain('<script>');
  });

  it('returns null for a block with no properties at all', () => {
    expect(renderFrontmatterCard(block('---\n---\nbody\n'))).toBeNull();
  });
});
