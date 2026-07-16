import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

const fetchNotePreviewMock = vi.fn();
const openNoteInEditorMock = vi.fn();

vi.mock('@/lib/note-actions', () => ({
  fetchNotePreview: (...args: unknown[]) => fetchNotePreviewMock(...args),
  openNoteInEditor: (...args: unknown[]) => openNoteInEditorMock(...args),
}));

vi.mock('@/lib/markdown', () => ({
  renderMarkdown: (s: string) => `<p data-md>${s}</p>`,
}));

import { WikilinkHoverPreview } from '../WikilinkHoverPreview';

function hover(el: Element): void {
  el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
}

function mountWithAnchor(html: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = html;
  document.body.appendChild(host);
  render(() => <WikilinkHoverPreview />, { container: document.body.appendChild(document.createElement('div')) });
  return host;
}

beforeEach(() => {
  vi.clearAllMocks();
  document.body.innerHTML = '';
});

describe('WikilinkHoverPreview', () => {
  it('shows a preview card with title, path, and rendered excerpt on hover', async () => {
    fetchNotePreviewMock.mockResolvedValue({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
      excerpt: 'Rust is great.',
    });

    const host = mountWithAnchor('<a data-note="rust">rust</a>');
    hover(host.querySelector('a[data-note]')!);

    await waitFor(() => {
      expect(document.querySelector('[data-testid="wikilink-preview"]')).not.toBeNull();
    });
    await waitFor(() => {
      const title = document.querySelector('[data-testid="wikilink-preview-title"]');
      expect(title?.textContent).toContain('Rust');
      expect(title?.textContent).toContain('notes/rust.md');
      const body = document.querySelector('[data-testid="wikilink-preview-body"]');
      expect(body?.innerHTML).toContain('Rust is great.');
    });
    expect(fetchNotePreviewMock).toHaveBeenCalledWith('rust', undefined);
  });

  it('does not show a card when hovering elements without data-note', async () => {
    const host = mountWithAnchor('<span>plain text</span>');
    hover(host.querySelector('span')!);

    await new Promise((r) => setTimeout(r, 450));
    expect(document.querySelector('[data-testid="wikilink-preview"]')).toBeNull();
    expect(fetchNotePreviewMock).not.toHaveBeenCalled();
  });

  it('shows a not-found state for unresolvable notes', async () => {
    fetchNotePreviewMock.mockResolvedValue(null);

    const host = mountWithAnchor('<a data-note="ghost">ghost</a>');
    hover(host.querySelector('a[data-note]')!);

    await waitFor(() => {
      const missing = document.querySelector('[data-testid="wikilink-preview-missing"]');
      expect(missing?.textContent).toContain('ghost');
    });
  });

  it('hides the card after hovering away', async () => {
    fetchNotePreviewMock.mockResolvedValue({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
      excerpt: 'x',
    });

    const host = mountWithAnchor('<a data-note="rust">rust</a><span>away</span>');
    hover(host.querySelector('a[data-note]')!);
    await waitFor(() => {
      expect(document.querySelector('[data-testid="wikilink-preview"]')).not.toBeNull();
    });

    hover(host.querySelector('span')!);
    await waitFor(() => {
      expect(document.querySelector('[data-testid="wikilink-preview"]')).toBeNull();
    });
  });

  it('opens the note in the editor when the card title is clicked', async () => {
    fetchNotePreviewMock.mockResolvedValue({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
      excerpt: 'x',
    });

    const host = mountWithAnchor('<a data-note="rust">rust</a>');
    hover(host.querySelector('a[data-note]')!);
    await waitFor(() => {
      expect(document.querySelector('[data-testid="wikilink-preview-title"]')).not.toBeNull();
    });

    (document.querySelector('[data-testid="wikilink-preview-title"]') as HTMLElement).click();
    expect(openNoteInEditorMock).toHaveBeenCalledWith('rust', undefined);
  });

  it('passes an explicit data-kiln through to preview resolution', async () => {
    fetchNotePreviewMock.mockResolvedValue(null);

    const host = mountWithAnchor('<a data-note="rust" data-kiln="/other-kiln">rust</a>');
    hover(host.querySelector('a[data-note]')!);

    await waitFor(() => {
      expect(fetchNotePreviewMock).toHaveBeenCalledWith('rust', '/other-kiln');
    });
  });

  it('renders no drag grip outside a DnD provider (tests, harness pages)', async () => {
    fetchNotePreviewMock.mockResolvedValue({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
      excerpt: 'x',
    });

    const host = mountWithAnchor('<a data-note="rust">rust</a>');
    hover(host.querySelector('a[data-note]')!);
    await waitFor(() => {
      expect(document.querySelector('[data-testid="wikilink-preview-title"]')).not.toBeNull();
    });
    expect(document.querySelector('[data-testid="wikilink-preview-drag"]')).toBeNull();
  });

  it('inside a DnD provider, the card registers a newTab draggable carrying the file tab', async () => {
    const { DragDropProvider, useDragDropContext } = await import('@thisbeyond/solid-dnd');
    fetchNotePreviewMock.mockResolvedValue({
      title: 'Rust',
      path: 'notes/rust.md',
      absPath: '/kiln/notes/rust.md',
      excerpt: 'x',
    });

    let registry: () => Record<string, { data?: { type?: string; tab?: { title: string; metadata?: { filePath?: string } } } }> = () => ({});
    const Probe = () => {
      const ctx = useDragDropContext()!;
      registry = () => ctx[0].draggables as ReturnType<typeof registry>;
      return null;
    };

    const host = document.createElement('div');
    host.innerHTML = '<a data-note="rust">rust</a>';
    document.body.appendChild(host);
    render(
      () => (
        <DragDropProvider>
          <Probe />
          <WikilinkHoverPreview />
        </DragDropProvider>
      ),
      { container: document.body.appendChild(document.createElement('div')) },
    );

    hover(host.querySelector('a[data-note]')!);
    await waitFor(() => {
      expect(document.querySelector('[data-testid="wikilink-preview-drag"]')).not.toBeNull();
    });

    const entry = registry()['hovercard:/kiln/notes/rust.md'];
    expect(entry).toBeTruthy();
    expect(entry.data?.type).toBe('newTab');
    expect(entry.data?.tab?.title).toBe('Rust');
    expect(entry.data?.tab?.metadata?.filePath).toBe('/kiln/notes/rust.md');
  });
});
