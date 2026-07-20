import { describe, expect, it } from 'vitest';
import {
  renderMarkdown,
  renderMarkdownDocAsync,
  renderPlainWithWikilinks,
} from '../markdown';

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

  it('renders markdown image syntax as an <img>', async () => {
    const html = await renderMarkdownDocAsync('![badge](https://example.com/b.svg)');
    expect(html).toContain('<img');
    expect(html).toContain('src="https://example.com/b.svg"');
  });
});
