import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { OilNode } from '../OilNode';
import { oilMountHtml } from '@/lib/markdown';
import { mountOilViews } from '../mount';
import fixture from './board.fixture.json';
import type { OilTree } from '@/lib/oil-types';

/**
 * The fixture is a REAL tree, captured from a running daemon rendering the
 * kanban plugin over five markdown tickets. It is not hand-written: the point
 * of these tests is that the renderer draws what Rust actually serializes, and
 * a hand-written tree would only prove it draws what this file believes.
 *
 * Re-capture with:
 *   curl -s -X POST localhost:PORT/api/plugins/kanban/view/board \
 *     -H 'content-type: application/json' \
 *     -d '{"params":{"kiln":"oil-board","folder":"tickets"}}'
 */
const TREE = (fixture as { node: OilTree }).node;

describe('OilNode', () => {
  it('draws a real captured tree', () => {
    const { container } = render(() => <OilNode node={TREE} />);
    const text = container.textContent ?? '';
    expect(text).toContain('TODO  (3)');
    expect(text).toContain('DOING  (1)');
    expect(text).toContain('DONE  (1)');
    expect(text).toContain('Cells are not pixels');
  });

  it('makes every action node an activatable control', () => {
    const { container } = render(() => <OilNode node={TREE} />);
    // Five tickets, five cards.
    expect(container.querySelectorAll('button').length).toBe(5);
  });

  it('hands the action name and params back verbatim', () => {
    const onAction = vi.fn();
    const { container } = render(() => <OilNode node={TREE} onAction={onAction} />);
    const first = container.querySelector('button');
    fireEvent.click(first!);
    expect(onAction).toHaveBeenCalledTimes(1);
    const [action, params] = onAction.mock.calls[0];
    expect(action).toBe('move');
    // The card's own next column, decided by the plugin — not by the renderer.
    expect(params).toMatchObject({ folder: 'tickets', kiln: 'oil-board', to: 'doing' });
    expect(params.file).toMatch(/\.md$/);
  });

  it('renders a row as a row and a column as a column', () => {
    const { container } = render(() => (
      <OilNode node={{ box: { direction: 'row', children: [{ text: { content: 'a' } }] } }} />
    ));
    const el = container.firstElementChild as HTMLElement;
    expect(el.style.flexDirection).toBe('row');
  });

  it('draws Empty as nothing, not as the word "empty"', () => {
    const { container } = render(() => <OilNode node="empty" />);
    expect(container.textContent).toBe('');
  });

  // The settings pane's rule, carried over: a construct added to Oil after
  // this renderer was written must look plain, never invisible.
  it('shows a placeholder for a node kind it has no renderer for', () => {
    const { container } = render(() => <OilNode node={{ hologram: { content: 'x' } }} />);
    expect(container.textContent).toContain('[hologram]');
  });

  it('applies bold and dim from the node style', () => {
    const { container } = render(() => (
      <OilNode node={{ text: { content: 'hi', style: { bold: true, dim: true } } }} />
    ));
    const span = container.querySelector('span') as HTMLElement;
    expect(span.style.fontWeight).toBe('600');
    expect(span.style.opacity).toBe('0.65');
  });
});

describe('the ```oil fence', () => {
  it('turns a plugin/view line into a mount point', () => {
    const html = oilMountHtml('kanban/board\n{ "folder": "tickets" }');
    expect(html).toContain('class="oil-mount"');
    expect(html).toContain('data-oil-plugin="kanban"');
    expect(html).toContain('data-oil-view="board"');
    expect(html).toContain('tickets');
  });

  it('accepts a fence with no params', () => {
    const html = oilMountHtml('kanban/board');
    expect(html).toContain('data-oil-view="board"');
    expect(html).toContain('data-oil-params="{}"');
  });

  it('renders a visible error for a target that is not plugin/view', () => {
    const html = oilMountHtml('kanban');
    expect(html).toContain('oil-error');
    expect(html).not.toContain('oil-mount');
  });

  it('renders a visible error for params that are not JSON', () => {
    const html = oilMountHtml('kanban/board\nfolder = tickets');
    expect(html).toContain('oil-error');
    expect(html).not.toContain('oil-mount');
  });
});

describe('mountOilViews', () => {
  it('mounts one view per placeholder and does not stack on a second pass', () => {
    const host = document.createElement('div');
    host.innerHTML = oilMountHtml('kanban/board');
    const disposeA = mountOilViews(host);
    const first = host.querySelectorAll('.oil-view').length;
    const disposeB = mountOilViews(host);
    expect(host.querySelectorAll('.oil-view').length).toBe(first);
    disposeA();
    disposeB();
  });

  it('ignores a placeholder with no plugin', () => {
    const host = document.createElement('div');
    host.innerHTML = '<div class="oil-mount"></div>';
    const dispose = mountOilViews(host);
    expect(host.querySelectorAll('.oil-view').length).toBe(0);
    dispose();
  });
});
