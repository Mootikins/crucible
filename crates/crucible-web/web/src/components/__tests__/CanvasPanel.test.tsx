import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { CanvasResponse } from '@/lib/canvas-types';

const getCanvasMock = vi.fn();
const saveCanvasMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getCanvas: (...args: unknown[]) => getCanvasMock(...args),
  saveCanvas: (...args: unknown[]) => saveCanvasMock(...args),
  rawFileUrl: (p: string) => `/api/file/raw?path=${encodeURIComponent(p)}`,
}));

import { CanvasPanel } from '../canvas/CanvasPanel';

const response = (over: Partial<CanvasResponse> = {}): CanvasResponse => ({
  kiln: '/kiln',
  rejected: [],
  canvas: {
    nodes: [
      { id: 'text-1', type: 'text', text: 'Hello **world**', x: 0, y: 0, width: 200, height: 120 },
      { id: 'file-1', type: 'file', file: 'Notes/A.md', x: 300, y: 0, width: 200, height: 120 },
      { id: 'link-1', type: 'link', url: 'https://example.com/x', x: 0, y: 200, width: 200, height: 100 },
      { id: 'group-1', type: 'group', label: 'Cluster', x: -40, y: -40, width: 600, height: 400 },
    ],
    edges: [
      { id: 'e1', fromNode: 'text-1', toNode: 'file-1', label: 'supersedes' },
    ],
  },
  ...over,
});

describe('CanvasPanel', () => {
  beforeEach(() => {
    getCanvasMock.mockReset();
    saveCanvasMock.mockReset();
    saveCanvasMock.mockResolvedValue(undefined);
  });

  it('renders every node type as DOM and edges as SVG', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);

    await waitFor(() => {
      expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(4);
    });

    const types = [...container.querySelectorAll('[data-testid="canvas-node"]')].map((n) =>
      n.getAttribute('data-node-type'),
    );
    expect(types).toEqual(['text', 'file', 'link', 'group']);

    // Edges are one SVG overlay, not per-node elements.
    expect(container.querySelectorAll('[data-testid="canvas-edges"]').length).toBe(1);
    await waitFor(() => {
      expect(container.querySelector('[data-testid="canvas-edge"]')).toBeTruthy();
    });
  });

  it('draws edge labels', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);

    await waitFor(() => {
      const label = container.querySelector('[data-testid="canvas-edge-label"]');
      expect(label?.textContent).toBe('supersedes');
    });
  });

  /**
   * The server strips a rejected reference and sends only the reason. The panel
   * must render a placeholder — and must not be able to show the path, because
   * it was never sent one.
   */
  it('quarantines a rejected node without revealing its path', async () => {
    getCanvasMock.mockResolvedValue(
      response({
        canvas: {
          nodes: [
            { id: 'bad', type: 'file', file: '', x: 0, y: 0, width: 200, height: 120 },
          ],
          edges: [],
        },
        rejected: [{ nodeId: 'bad', reason: 'reference escapes the kiln via a parent-directory component' }],
      }),
    );

    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);

    await waitFor(() => {
      const quarantined = container.querySelector('[data-testid="canvas-node-quarantined"]');
      expect(quarantined).toBeTruthy();
      expect(quarantined!.textContent).toContain('Reference blocked');
    });

    expect(container.innerHTML).not.toContain('etc/passwd');
    expect(container.innerHTML).not.toContain('..');
  });

  it('surfaces a load failure instead of rendering an empty canvas', async () => {
    getCanvasMock.mockRejectedValue(new Error('Canvas is not within an open kiln'));
    const { findByTestId } = render(() => <CanvasPanel filePath="/outside/Board.canvas" />);

    const err = await findByTestId('canvas-error');
    expect(err.textContent).toContain('not within an open kiln');
  });

  it('renders a link node as a card that opens in a new tab', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);

    await waitFor(() => {
      const link = container.querySelector('[data-testid="canvas-link-node"]') as HTMLAnchorElement;
      expect(link).toBeTruthy();
      // An iframe would contact the third party on open; a card does not.
      expect(link.tagName).toBe('A');
      expect(link.rel).toContain('noopener');
    });
    expect(container.querySelector('iframe')).toBeNull();
  });

  it('shows the empty canvas without error', async () => {
    getCanvasMock.mockResolvedValue(response({ canvas: { nodes: [], edges: [] } }));
    const { container } = render(() => <CanvasPanel filePath="/kiln/Empty.canvas" />);

    await waitFor(() => {
      expect(container.querySelector('[data-testid="canvas-surface"]')).toBeTruthy();
    });
    expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(0);
    expect(container.querySelector('[data-testid="canvas-error"]')).toBeNull();
  });
});
