import { describe, expect, it } from 'vitest';
import {
  proseClass,
  renderMarkdown,
  renderMarkdownDocAsync,
  renderPlainWithWikilinks,
} from '../markdown';
import { applyTheme } from '../theme';

describe('renderPlainWithWikilinks (user bubbles)', () => {
  it('turns a user-authored [[link]] into a .wikilink anchor', () => {
    const html = renderPlainWithWikilinks('see [[My Note]] please');
    expect(html).toContain('class="wikilink"');
    expect(html).toContain('data-note="My Note"');
    expect(html).toContain('see ');
  });

  it('escapes surrounding HTML (no markdown, no injection)', () => {
    const html = renderPlainWithWikilinks('<b>hi</b> [[N]]');
    expect(html).toContain('&lt;b&gt;hi&lt;/b&gt;');
    expect(html).not.toContain('<b>hi</b>');
    expect(html).toContain('data-note="N"');
  });
});

describe('markdown renderer', () => {
  it('renders strong tags for bold markdown', () => {
    const html = renderMarkdown('**bold**');
    expect(html).toContain('<strong>bold</strong>');
  });

  it('renders crucible wikilink anchors', () => {
    const html = renderMarkdown('[[My Note]]');
    expect(html).toContain('class="wikilink"');
    expect(html).toContain('data-note="My Note"');
  });

  it('aliased wikilinks display the alias but resolve the target', () => {
    const html = renderMarkdown('[[My Note|shown text]]');
    expect(html).toContain('data-note="My Note"');
    expect(html).toContain('>shown text</a>');
    expect(html).not.toContain('data-note="My Note|shown text"');
  });

  it('heading and block fragments are stripped from the resolution target', () => {
    expect(renderMarkdown('[[My Note#Section]]')).toContain('data-note="My Note"');
    expect(renderMarkdown('[[My Note#^block-id]]')).toContain('data-note="My Note"');
  });

  it('renders heading markdown', () => {
    const html = renderMarkdown('# Hello');
    expect(html).toContain('<h1>Hello</h1>');
  });

  it('sanitizes unsafe script tags', () => {
    const html = renderMarkdown('<script>alert(1)</script>');
    expect(html).not.toContain('<script>');
  });

  it('renders GFM task lists as checkboxes with state', () => {
    const html = renderMarkdown('- [ ] todo\n- [x] done\n- plain');
    expect(html).toContain('class="task-list-item"');
    expect(html).toContain('type="checkbox"');
    // The done item is checked; the todo item is not.
    expect(html).toMatch(/checkbox"[^>]*checked/);
    // The literal brackets are gone from the item text.
    expect(html).not.toContain('[ ] todo');
    expect(html).not.toContain('[x] done');
    expect(html).toContain('todo');
    expect(html).toContain('done');
    // A plain list item is untouched.
    expect(html).toContain('<li>plain</li>');
  });

  it('does NOT render raw HTML in the chat/hover path', () => {
    // Chat and hover keep html:false — a centered block stays inert text.
    const html = renderMarkdown('<p align="center">hi</p>');
    expect(html).not.toContain('<p align="center">');
  });
});

describe('renderMarkdownDocAsync (reading view)', () => {
  it('renders embedded HTML like a centered demo block (sanitized)', async () => {
    const html = await renderMarkdownDocAsync(
      '<p align="center"><img src="assets/demo.gif" alt="demo" width="720" /></p>',
    );
    expect(html).toContain('align="center"');
    expect(html).toContain('<img');
    expect(html).toContain('alt="demo"');
  });

  it('still strips scripts even with raw HTML enabled', async () => {
    const html = await renderMarkdownDocAsync('<p>ok</p><script>alert(1)</script>');
    expect(html).toContain('<p>ok</p>');
    expect(html).not.toContain('<script>');
  });

  it('wraps code blocks with a copy button', async () => {
    const html = await renderMarkdownDocAsync('```sh\nnpm install\n```');
    expect(html).toContain('md-codeblock');
    expect(html).toContain('data-copy');
    expect(html).toContain('<pre');
  });

  it('highlights a code block for BOTH themes in one pass', async () => {
    // The reading view caches this HTML string, so it cannot re-highlight when
    // the theme flips — every token has to carry its light color along as
    // `--shiki-light`, and that declaration has to survive the sanitizer.
    const html = await renderMarkdownDocAsync('```ts\nconst x = 1;\n```');
    expect(html).toContain('--shiki-light:');
  });

  it('renders markdown image syntax as an <img>', async () => {
    const html = await renderMarkdownDocAsync('![badge](https://example.com/b.svg)');
    expect(html).toContain('<img');
    expect(html).toContain('src="https://example.com/b.svg"');
  });
});

describe('proseClass', () => {
  it('inverts the typography greys for dark ONLY', () => {
    // `prose-invert` is the one part of the prose class that is not a shell
    // token: the typography plugin swaps in its own light greys, so leaving it
    // on in the light theme paints #d1d5db body text and WHITE inline code
    // onto a white ground.
    applyTheme('dark');
    expect(proseClass()).toContain('prose-invert');
    applyTheme('light');
    expect(proseClass()).not.toContain('prose-invert');
    applyTheme('dark');
  });
});
