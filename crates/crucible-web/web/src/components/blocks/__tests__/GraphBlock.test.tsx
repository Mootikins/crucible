import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';

/**
 * `GraphBlock` is the plan's test of a **parameterised read over a command**,
 * so what these lock down is the argument path, not the picture.
 *
 * The block re-issues the RPC every time an argument moves. That is the
 * property with a cost, and it is the one that has to be observable: a test
 * that only asserted the first render would pass with the depth control wired
 * to nothing.
 *
 * `vi.hoisted`, because `vi.mock`'s factory is hoisted above ordinary
 * top-level consts.
 */
const mocks = vi.hoisted(() => ({
  runPluginCommand: vi.fn(),
  listKilns: vi.fn(),
  activeFile: vi.fn<() => string | null>(),
  openFileInEditor: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  runPluginCommand: mocks.runPluginCommand,
  listKilns: mocks.listKilns,
}));

vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: mocks.openFileInEditor,
}));

vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({ activeFile: mocks.activeFile }),
}));

import { GraphBlock } from '../GraphBlock';

const NEIGHBOURHOOD = {
  root: 'Meta/Canvas.md',
  depth: 1,
  total: 2,
  truncated: false,
  rings: [{ hops: 1, paths: ['Meta/Oil.md', 'Help/Wikilinks.md'] }],
};

beforeEach(() => {
  mocks.listKilns.mockResolvedValue([{ path: '/vault' }]);
  mocks.activeFile.mockReturnValue(null);
  mocks.runPluginCommand.mockResolvedValue(NEIGHBOURHOOD);
});

describe('GraphBlock', () => {
  it('reads the fence-named note and renders one section per hop', async () => {
    mocks.runPluginCommand.mockResolvedValue({
      root: 'Meta/Canvas.md',
      depth: 2,
      total: 3,
      truncated: false,
      rings: [
        { hops: 1, paths: ['Meta/Oil.md'] },
        { hops: 2, paths: ['Help/Wikilinks.md', 'Help/Tags.md'] },
      ],
    });

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md', depth: 2 }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Oil.md'));
    expect(mocks.runPluginCommand).toHaveBeenCalledWith(
      'graph_neighborhood',
      { path: 'Meta/Canvas.md', depth: 2 },
      'graph',
    );
    expect(container.textContent).toContain('1 hop (1)');
    expect(container.textContent).toContain('2 hops (2)');
  });

  // The whole point of the step. A depth control wired to nothing, or one that
  // filters an already-fetched answer in the browser, passes every other
  // assertion in this file.
  it('re-invokes the command when the depth control moves', async () => {
    const { getByTestId, container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Oil.md'));
    expect(mocks.runPluginCommand).toHaveBeenCalledTimes(1);

    fireEvent.input(getByTestId('graph-depth'), { target: { value: '3' } });

    await waitFor(() =>
      expect(mocks.runPluginCommand).toHaveBeenLastCalledWith(
        'graph_neighborhood',
        { path: 'Meta/Canvas.md', depth: 3 },
        'graph',
      ),
    );
  });

  // The read costs a round trip per move, so the block says what each one
  // cost. The graph evidence in `docs/Meta/Analysis/Plugin API Plan.md` keeps it in
  // front of whoever is deciding whether this shape is worth keeping.
  it('reports the round-trip time of the read', async () => {
    const { getByTestId } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(getByTestId('graph-latency').textContent).toMatch(/\d+ ms/));
  });

  // No fence path: the block follows the editor, and the absolute buffer path
  // becomes the kiln-relative one a note record carries.
  it('follows the focused note, kiln-relative', async () => {
    mocks.activeFile.mockReturnValue('/vault/Meta/Canvas.md');

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{}} />
    ));

    await waitFor(() =>
      expect(mocks.runPluginCommand).toHaveBeenCalledWith(
        'graph_neighborhood',
        { path: 'Meta/Canvas.md', depth: 1 },
        'graph',
      ),
    );
    expect(container.textContent).toContain('Canvas.md');
  });

  it('asks for nothing when no note has focus', async () => {
    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{}} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Open a note'));
    expect(mocks.runPluginCommand).not.toHaveBeenCalled();
  });

  // The plugin answers in one shape whether the read worked or not, so a
  // failure must reach the reader instead of rendering as an empty list.
  it('shows the error the plugin reported', async () => {
    mocks.runPluginCommand.mockResolvedValue({
      root: 'Meta/Canvas.md',
      depth: 1,
      total: 0,
      truncated: false,
      rings: [],
      error: 'no kiln is open',
    });

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('no kiln is open'));
  });

  it('says an isolated note has no neighbours rather than drawing nothing', async () => {
    mocks.runPluginCommand.mockResolvedValue({
      root: 'Meta/Canvas.md',
      depth: 1,
      total: 0,
      truncated: false,
      rings: [],
    });

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Nothing links to or from'));
  });

  it('says when the answer was cut short', async () => {
    mocks.runPluginCommand.mockResolvedValue({
      root: 'Meta/Canvas.md',
      depth: 3,
      total: 2,
      truncated: true,
      rings: [{ hops: 1, paths: ['Meta/Oil.md', 'Help/Tags.md'] }],
    });

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Cut short'));
  });

  // The block declares which plugin it draws for. Without the third argument
  // `runPluginCommand` defaults to `APP_CALLER`, so the block reaches the
  // route as the app and the per-plugin comparison never runs in production.
  // The Rust tests would still pass, because they send the header by hand.
  //
  // Asserted, not proved: `props.plugin` comes from the fence's first line,
  // so a note author chose it. See `routes/plugin_caller.rs`.
  it('declares itself as the plugin it draws for', async () => {
    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(mocks.runPluginCommand).toHaveBeenCalled());
    // The third positional argument is the caller. Read it off the call
    // rather than matching the whole call, so a change to the argument
    // object does not silently take this assertion with it.
    expect(mocks.runPluginCommand.mock.calls[0][2]).toBe('graph');
    expect(container).toBeTruthy();
  });

  it('opens a neighbour at its absolute path', async () => {
    mocks.activeFile.mockReturnValue('/vault/Meta/Canvas.md');

    const { findByTitle } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{}} />
    ));

    fireEvent.click(await findByTitle('Meta/Oil.md'));
    expect(mocks.openFileInEditor).toHaveBeenCalledWith('/vault/Meta/Oil.md', 'Oil.md');
  });
});
