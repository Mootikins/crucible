import { describe, expect, it } from 'vitest';
import { renderMarkdown } from '../markdown';

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

  it('renders heading markdown', () => {
    const html = renderMarkdown('# Hello');
    expect(html).toContain('<h1>Hello</h1>');
  });

  it('sanitizes unsafe script tags', () => {
    const html = renderMarkdown('<script>alert(1)</script>');
    expect(html).not.toContain('<script>');
  });
});
