import { describe, expect, it } from 'vitest';
import { renderMarkdown, renderPlainWithWikilinks } from '../markdown';

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
});
