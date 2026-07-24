import { describe, it, expect, vi, beforeEach } from 'vitest';

// Real mermaid needs a browser (layout/measurement); mock the renderer and
// assert the markdown pipeline's placeholder → diagram/fallback wiring.
const renderMermaidMock = vi.fn();
vi.mock('../mermaid', () => ({
  renderMermaid: (...args: unknown[]) => renderMermaidMock(...args),
  // Real refit needs layout (getBBox); jsdom has none, so pass through — the
  // fitting itself is covered by the browser-level render check.
  fitMermaidViewBox: (svg: string) => svg,
}));

import { renderMarkdownChatAsync } from '../markdown';

const MERMAID_DOC = ['```mermaid', 'graph TD;', 'A-->B;', '```'].join('\n');

beforeEach(() => {
  vi.clearAllMocks();
});

describe('mermaid pipeline', () => {
  it('replaces a ```mermaid fence with a rendered diagram', async () => {
    renderMermaidMock.mockResolvedValue('<svg data-testid="fake"><g/></svg>');
    const html = await renderMarkdownChatAsync(MERMAID_DOC);
    expect(renderMermaidMock).toHaveBeenCalledOnce();
    // The mermaid source reached the renderer (newlines preserved).
    expect(renderMermaidMock.mock.calls[0][0]).toContain('graph TD;');
    expect(renderMermaidMock.mock.calls[0][0]).toContain('A-->B;');
    expect(html).toContain('class="mermaid-diagram"');
    expect(html).toContain('<svg');
    // Not left as a raw code block.
    expect(html).not.toContain('language-mermaid');
  });

  it('falls back to the source when a diagram fails to render', async () => {
    renderMermaidMock.mockResolvedValue(null);
    const html = await renderMarkdownChatAsync(MERMAID_DOC);
    expect(html).toContain('mermaid-error');
    expect(html).toContain('graph TD;');
  });

  it('never invokes mermaid for a plain code block', async () => {
    renderMermaidMock.mockResolvedValue('<svg/>');
    const html = await renderMarkdownChatAsync('```js\nconst x = 1;\n```');
    expect(renderMermaidMock).not.toHaveBeenCalled();
    expect(html).not.toContain('mermaid-diagram');
  });
});
