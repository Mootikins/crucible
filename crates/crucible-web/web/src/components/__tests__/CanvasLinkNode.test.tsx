import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { CanvasResponse } from '@/lib/canvas-types';

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
