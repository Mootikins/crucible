import { describe, expect, it } from 'vitest';
import { renderMarkdownDocAsync, renderMarkdown } from '@/lib/markdown';

/**
 * Link resolution reads the nearest `data-kiln` ancestor. The document renderer
 * allows raw HTML, so without an explicit rule a NOTE can carry its own
 * `data-kiln` and redirect where its links resolve — content-driven, needing no
 * script and no access to the page. `data-note` and `data-copy` are the only
 * data attributes the app relies on and stay allowed.
 */
describe('data attribute injection', () => {
  it('strips a kiln marker supplied by note content', async () => {
    const out = await renderMarkdownDocAsync(
      '<a data-note="secret" data-kiln="/other/kiln">click</a>',
      undefined,
    );
    expect(out).toContain('data-note');
    expect(out, 'note content must not choose the kiln its links resolve in').not.toContain(
      'data-kiln',
    );
  });

  it('strips arbitrary data attributes but keeps the two the app uses', async () => {
    const out = await renderMarkdownDocAsync(
      '<a data-note="x" data-copy="y" data-evil="z">click</a>',
      undefined,
    );
    expect(out).toContain('data-note');
    expect(out).toContain('data-copy');
    expect(out).not.toContain('data-evil');
  });

  it('leaves the inline renderer escaping raw HTML as before', () => {
    // Escaped, so the attribute is inert text rather than markup — asserting
    // the substring is absent would be wrong, since the escaped form contains
    // it verbatim.
    const out = renderMarkdown('<a data-kiln="/other">x</a>');
    expect(out).toContain('&lt;a');
    expect(out).not.toContain('<a data-kiln');
  });
});
