import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
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
  activeFile: vi.fn<() => string | null>(),
  openFileInEditor: vi.fn(),
}));

// `runPluginCommand` is NOT stubbed: the block issues it against the
// `POST /api/plugins/command` route below, so the command name, the arguments
// and the caller it travels under are read off the wire — which is the argument
// path this file exists to pin.
/** Every command call, as it went out. */
const sent: { name: string; args: unknown; caller: string }[] = [];
/** What the command route answers next. */
let reply: unknown = {};

vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: mocks.openFileInEditor,
}));

vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({ activeFile: mocks.activeFile }),
}));

import { GraphBlock } from '../GraphBlock';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';
import { PLUGIN_CALLER_HEADER } from '@/lib/api';

const KILNS = [{ path: '/vault', name: 'vault' }];

let env: TestQueryEnv;

const NEIGHBOURHOOD = {
  root: 'Meta/Canvas.md',
  depth: 1,
  total: 2,
  truncated: false,
  rings: [{ hops: 1, paths: ['Meta/Oil.md', 'Help/Wikilinks.md'] }],
};

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  // The roster paints on the first render, the way a reload does; the fetch
  // below runs and answers the same list.
  localStorage.setItem('crucible:cache:kilns', JSON.stringify(KILNS));
  sent.length = 0;
  reply = NEIGHBOURHOOD;
  mocks.activeFile.mockReturnValue(null);
  env = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: KILNS }),
    'POST /api/plugins/command': async (request) => {
      const { name, args } = (await request.json()) as { name: string; args: unknown };
      sent.push({ name, args, caller: request.headers.get(PLUGIN_CALLER_HEADER) ?? '' });
      return reply;
    },
  });
});

afterEach(() => {
  env.restore();
  resetKilnsForTests();
});

describe('GraphBlock', () => {
  it('reads the fence-named note and renders one section per hop', async () => {
    reply = {
      root: 'Meta/Canvas.md',
      depth: 2,
      total: 3,
      truncated: false,
      rings: [
        { hops: 1, paths: ['Meta/Oil.md'] },
        { hops: 2, paths: ['Help/Wikilinks.md', 'Help/Tags.md'] },
      ],
    };

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md', depth: 2 }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Oil.md'));
    // Name, args and the caller it travels under, read off the wire.
    expect(sent[0]).toEqual({
      name: 'graph_neighborhood',
      args: { path: 'Meta/Canvas.md', depth: 2 },
      caller: 'graph',
    });
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
    expect(sent).toHaveLength(1);

    fireEvent.input(getByTestId('graph-depth'), { target: { value: '3' } });

    await waitFor(() =>
      expect(sent.at(-1)).toEqual({
        name: 'graph_neighborhood',
        args: { path: 'Meta/Canvas.md', depth: 3 },
        caller: 'graph',
      }),
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
      expect(sent[0]).toEqual({
        name: 'graph_neighborhood',
        args: { path: 'Meta/Canvas.md', depth: 1 },
        caller: 'graph',
      }),
    );
    expect(container.textContent).toContain('Canvas.md');
  });

  it('asks for nothing when no note has focus', async () => {
    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{}} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Open a note'));
    expect(sent).toHaveLength(0);
  });

  // The plugin answers in one shape whether the read worked or not, so a
  // failure must reach the reader instead of rendering as an empty list.
  it('shows the error the plugin reported', async () => {
    reply = {
      root: 'Meta/Canvas.md',
      depth: 1,
      total: 0,
      truncated: false,
      rings: [],
      error: 'no kiln is open',
    };

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('no kiln is open'));
  });

  // The command route answers a bare JSON value, so nothing between the
  // daemon and this block vouches for the shape. A reply that is not a
  // Neighborhood must be refused, not cast: before the decode, the cast
  // rendered this fixture as a plausible empty neighbourhood.
  it('refuses a malformed reply instead of rendering it', async () => {
    reply = {
      root: 'Meta/Canvas.md',
      depth: 1,
      total: 0,
      truncated: false,
    };

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('malformed graph_neighborhood reply'));
    expect(container.textContent).not.toContain('Nothing links to or from');
  });

  it('says an isolated note has no neighbours rather than drawing nothing', async () => {
    reply = {
      root: 'Meta/Canvas.md',
      depth: 1,
      total: 0,
      truncated: false,
      rings: [],
    };

    const { container } = render(() => (
      <GraphBlock plugin="graph" block="neighborhood" params={{ path: 'Meta/Canvas.md' }} />
    ));

    await waitFor(() => expect(container.textContent).toContain('Nothing links to or from'));
  });

  it('says when the answer was cut short', async () => {
    reply = {
      root: 'Meta/Canvas.md',
      depth: 3,
      total: 2,
      truncated: true,
      rings: [{ hops: 1, paths: ['Meta/Oil.md', 'Help/Tags.md'] }],
    };

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

    await waitFor(() => expect(sent).toHaveLength(1));
    // The caller is read off the header rather than the whole call, so a
    // change to the argument object does not silently take this assertion
    // with it.
    expect(sent[0]!.caller).toBe('graph');
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
