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

    // Groups paint first, then edges, then cards. A group is a backdrop, and
    // once it gained a background it hid every connection crossing it — so
    // render order is banded, and document order (which IS z-order in the
    // format) is preserved only within the card band.
    const types = [...container.querySelectorAll('[data-testid="canvas-node"]')].map((n) =>
      n.getAttribute('data-node-type'),
    );
    expect(types[0], 'groups render behind everything else').toBe('group');
    expect(types.slice(1)).toEqual(['text', 'file', 'link']);

    const layer = container.querySelector('[data-testid="canvas-layer"]')!;
    const order = [...layer.children].map((c) => c.getAttribute('data-testid') ?? c.tagName);
    expect(
      order.indexOf('canvas-edges'),
      'edges must paint above groups so a group background cannot hide them',
    ).toBeGreaterThan(0);

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

  /**
   * The shortcuts were bound to `window`, so Delete and Ctrl+Z fired while the
   * user was focused on the file tree or another pane, and every open canvas
   * tab acted on one keypress. They live on the surface now.
   */
  it('ignores destructive shortcuts pressed outside the canvas surface', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);
    await waitFor(() => {
      expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(4);
    });

    // Select a node first — with an empty selection Delete is a no-op whether
    // or not the handler fires, which would make this test prove nothing.
    const node = container.querySelector('[data-testid="canvas-node"]') as HTMLElement;
    node.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1 }));
    await waitFor(() => {
      expect(container.querySelector('[data-testid="canvas-resize-handle"]')).toBeTruthy();
    });

    const outsider = document.createElement('button');
    document.body.appendChild(outsider);
    outsider.focus();
    outsider.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }));

    expect(
      container.querySelectorAll('[data-testid="canvas-node"]').length,
      'a keypress outside the canvas must not delete its nodes',
    ).toBe(4);
    outsider.remove();
  });

  /**
   * The panel is disposed on every tab switch. Cancelling the debounce instead
   * of flushing it loses any edit made inside the window, silently.
   */
  it('flushes a pending save when the panel unmounts', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container, unmount } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);
    const surface = (await waitFor(() => {
      const el = container.querySelector('[data-testid="canvas-surface"]');
      expect(el).toBeTruthy();
      return el;
    })) as HTMLElement;

    // Double-click empty space creates a card, which marks the doc dirty.
    surface.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await waitFor(() => {
      expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(5);
    });
    expect(saveCanvasMock).not.toHaveBeenCalled();

    unmount();

    await waitFor(() => expect(saveCanvasMock).toHaveBeenCalled());
  });

  /**
   * Selection and editing are separate. A single click focuses a card so it can
   * be scrolled and styled; only a double click opens the editor. Conflating
   * them meant merely selecting a card stole the keyboard.
   */
  it('selects on single click and only edits on double click', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);
    await waitFor(() => {
      expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(4);
    });

    const card = container.querySelector('[data-node-id="text-1"]') as HTMLElement;
    card.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1 }));

    await waitFor(() => {
      expect(container.querySelector('[data-testid="canvas-card-toolbar"]')).toBeTruthy();
    });
    expect(container.querySelectorAll('[data-testid="canvas-resize-handle"]').length).toBe(4);

    // A real double click lands on the CONTENT inside the card, not on the
    // node box itself.
    const inner = card.querySelector('[data-canvas-scroll]') as HTMLElement;
    inner.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));

    await waitFor(() => {
      expect(container.querySelector('[data-testid="canvas-card-toolbar"]')).toBeNull();
    });
    expect(
      container.querySelectorAll('[data-testid="canvas-node"]').length,
      'double clicking a card must edit it, not create a new card',
    ).toBe(4);
  });

  /** Connector dots live at edge midpoints, separate from the corner handles. */
  it('offers four corner handles and four edge connectors on a selected card', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);
    await waitFor(() => {
      expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(4);
    });

    const card = container.querySelector('[data-node-id="file-1"]') as HTMLElement;
    card.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1 }));

    await waitFor(() => {
      const corners = [...container.querySelectorAll('[data-testid="canvas-resize-handle"]')].map(
        (el) => el.getAttribute('data-corner'),
      );
      expect(corners.sort()).toEqual(['ne', 'nw', 'se', 'sw']);
    });

    const sides = [...container.querySelectorAll('[data-testid="canvas-connect-zone"]')].map((el) =>
      el.getAttribute('data-side'),
    );
    expect(sides.sort()).toEqual(['bottom', 'left', 'right', 'top']);
  });

  /**
   * Dragging must MOVE a card, not rebuild it.
   *
   * The document is immutable — every edit returns fresh node objects, which is
   * what makes undo a stack of snapshots — but a reference-keyed `<For>` turned
   * that into a full teardown and remount of every card on every drag frame.
   * Note cards refetched their file each time, which is what the "Loading…"
   * flicker was. Keying by id keeps the element identity so only the position
   * bindings update.
   */
  it('moves a card on drag without recreating its DOM element', async () => {
    getCanvasMock.mockResolvedValue(response());
    const { container } = render(() => <CanvasPanel filePath="/kiln/Board.canvas" />);
    await waitFor(() => {
      expect(container.querySelectorAll('[data-testid="canvas-node"]').length).toBe(4);
    });

    const before = container.querySelector('[data-node-id="text-1"]') as HTMLElement;
    const beforeLeft = before.style.left;

    before.dispatchEvent(
      new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, clientX: 10, clientY: 10 }),
    );
    const surface = container.querySelector('[data-testid="canvas-surface"]')!;
    surface.dispatchEvent(
      new PointerEvent('pointermove', { bubbles: true, pointerId: 1, clientX: 90, clientY: 10 }),
    );
    surface.dispatchEvent(
      new PointerEvent('pointerup', { bubbles: true, pointerId: 1, clientX: 90, clientY: 10 }),
    );

    const after = container.querySelector('[data-node-id="text-1"]') as HTMLElement;
    expect(after, 'the card element must survive a drag, not be rebuilt').toBe(before);
    expect(after.style.left, 'but its position must have changed').not.toBe(beforeLeft);
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
