import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, waitFor } from '@solidjs/testing-library';
import type { CanvasResponse } from '@/lib/canvas-types';
import { normaliseUrl } from '../canvas/LinkPrompt';

const getCanvasMock = vi.fn();
vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getCanvas: (...a: unknown[]) => getCanvasMock(...a),
  saveCanvas: vi.fn(),
  rawFileUrl: (p: string) => `/api/file/raw?path=${encodeURIComponent(p)}`,
}));

import { CanvasPanel } from '../canvas/CanvasPanel';

const withUrl = (url: string): CanvasResponse => ({
  kiln: '/kiln',
  rejected: [],
  canvas: {
    nodes: [{ id: 'l', type: 'link', url, x: 0, y: 0, width: 200, height: 100 }],
    edges: [],
  },
});

/**
 * A link node's URL is arbitrary document text and containment does not police
 * it — a URL is not a filesystem reference. But it lands in an `href`.
 */
describe('canvas link nodes', () => {
  beforeEach(() => getCanvasMock.mockReset());

  it.each(['javascript:alert(1)', 'data:text/html,<script>alert(1)</script>', 'vbscript:msgbox'])(
    'refuses to navigate to %s',
    async (url) => {
      getCanvasMock.mockResolvedValue(withUrl(url));
      const { container } = render(() => <CanvasPanel filePath="/kiln/B.canvas" />);

      const link = (await waitFor(() => {
        const el = container.querySelector('[data-testid="canvas-link-node"]');
        expect(el).toBeTruthy();
        return el;
      })) as HTMLAnchorElement;

      expect(link.getAttribute('href'), `${url} must not be navigable`).toBeNull();
      expect(link.getAttribute('data-unsafe-scheme')).toBe('true');
    },
  );

  it('still navigates ordinary web links', async () => {
    getCanvasMock.mockResolvedValue(withUrl('https://jsoncanvas.org'));
    const { container } = render(() => <CanvasPanel filePath="/kiln/B.canvas" />);

    const link = (await waitFor(() => {
      const el = container.querySelector('[data-testid="canvas-link-node"]');
      expect(el).toBeTruthy();
      return el;
    })) as HTMLAnchorElement;

    expect(link.getAttribute('href')).toBe('https://jsoncanvas.org');
    expect(link.rel).toContain('noopener');
  });
});

/**
 * The renderer already refuses to navigate a dangerous scheme, but rejecting it
 * at creation means such a URL never reaches the document at all — better than
 * writing one to disk and relying on every future reader to keep it inert.
 */
describe('normaliseUrl', () => {
  it.each([
    ['jsoncanvas.org', 'https://jsoncanvas.org/'],
    ['https://example.com/a?b=1', 'https://example.com/a?b=1'],
    ['http://localhost:3000/x', 'http://localhost:3000/x'],
  ])('accepts %s', (input, expected) => {
    expect(normaliseUrl(input)).toBe(expected);
  });

  it.each([
    'javascript:alert(1)',
    'data:text/html,<script>alert(1)</script>',
    'vbscript:msgbox',
    'file:///etc/passwd',
    'mailto:someone@example.com',
    '   ',
    '',
  ])('refuses %s', (input) => {
    expect(normaliseUrl(input)).toBeNull();
  });
});

describe('authoring a web card', () => {
  const empty = (): CanvasResponse => ({
    kiln: '/kiln',
    rejected: [],
    canvas: { nodes: [], edges: [] },
  });

  beforeEach(() => getCanvasMock.mockReset());

  const paste = async (text: string) => {
    getCanvasMock.mockResolvedValue(empty());
    const { container } = render(() => <CanvasPanel filePath="/kiln/B.canvas" />);
    const surface = await waitFor(() => {
      const el = container.querySelector('[data-testid="canvas-surface"]');
      expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    fireEvent.paste(surface, {
      clipboardData: { getData: () => text },
    });
    return container;
  };

  it('creates a web card from a pasted URL, as Obsidian does', async () => {
    const container = await paste('https://jsoncanvas.org');

    await waitFor(() => {
      const link = container.querySelector('[data-testid="canvas-link-node"]') as HTMLAnchorElement;
      expect(link).toBeTruthy();
      expect(link.getAttribute('href')).toBe('https://jsoncanvas.org/');
    });
  });

  it('leaves ordinary pasted text alone rather than making a card of it', async () => {
    const container = await paste('just some notes I copied');

    expect(container.querySelector('[data-testid="canvas-link-node"]')).toBeNull();
  });

  it('will not build a card from a dangerous scheme', async () => {
    const container = await paste('javascript:alert(1)');

    expect(container.querySelector('[data-testid="canvas-link-node"]')).toBeNull();
  });

  const drop = async (data: Record<string, string>) => {
    getCanvasMock.mockResolvedValue(empty());
    const { container } = render(() => <CanvasPanel filePath="/kiln/B.canvas" />);
    const surface = await waitFor(() => {
      const el = container.querySelector('[data-testid="canvas-surface"]');
      expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    fireEvent.drop(surface, {
      clientX: 100,
      clientY: 100,
      dataTransfer: {
        types: Object.keys(data),
        getData: (type: string) => data[type] ?? '',
      },
    });
    return container;
  };

  it('creates a web card from a link dragged out of the browser', async () => {
    const container = await drop({ 'text/uri-list': 'https://jsoncanvas.org' });

    await waitFor(() => {
      const link = container.querySelector('[data-testid="canvas-link-node"]') as HTMLAnchorElement;
      expect(link).toBeTruthy();
      expect(link.getAttribute('href')).toBe('https://jsoncanvas.org/');
    });
  });

  /** `text/uri-list` is a list and the spec allows `#` comment lines. */
  it('skips comment lines in a uri-list payload', async () => {
    const container = await drop({
      'text/uri-list': '# a comment\r\nhttps://example.com/page\r\n',
    });

    await waitFor(() => {
      const link = container.querySelector('[data-testid="canvas-link-node"]') as HTMLAnchorElement;
      expect(link).toBeTruthy();
      expect(link.getAttribute('href')).toBe('https://example.com/page');
    });
  });

  it('ignores a drag that carries no URL', async () => {
    const container = await drop({ 'text/plain': 'some dragged prose' });

    expect(container.querySelector('[data-testid="canvas-link-node"]')).toBeNull();
  });
});
