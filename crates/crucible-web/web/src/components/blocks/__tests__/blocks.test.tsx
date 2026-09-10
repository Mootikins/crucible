import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { pluginMountHtml } from '@/lib/markdown';
import { mountPluginBlocks } from '../mount';
import { GenericBlock } from '../GenericBlock';
import { KanbanBlock } from '../KanbanBlock';
import { lookupBlock, registerBlock } from '../registry';

// EventSource does not exist in jsdom, and usePublication opens one. A stub is
// enough: these tests exercise the first read and the render, not the push.
class StubEventSource {
  addEventListener() {}
  close() {}
}
beforeEach(() => {
  vi.stubGlobal('EventSource', StubEventSource);
});

describe('the ```plugin fence', () => {
  it('turns a plugin/block line into a mount point', () => {
    const html = pluginMountHtml('kanban/board\n{ "folder": "tickets" }');
    expect(html).toContain('class="plugin-mount"');
    expect(html).toContain('data-plugin-name="kanban"');
    expect(html).toContain('data-plugin-block="board"');
    expect(html).toContain('tickets');
  });

  it('accepts a fence with no params', () => {
    const html = pluginMountHtml('kanban/board');
    expect(html).toContain('data-plugin-params="{}"');
  });

  it('renders a visible error for a target that is not plugin/block', () => {
    const html = pluginMountHtml('kanban');
    expect(html).toContain('plugin-block-error');
    expect(html).not.toContain('plugin-mount');
  });

  it('renders a visible error for params that are not JSON', () => {
    const html = pluginMountHtml('kanban/board\nfolder = tickets');
    expect(html).toContain('plugin-block-error');
    expect(html).not.toContain('plugin-mount');
  });
});

describe('mountPluginBlocks', () => {
  it('mounts once per placeholder and does not stack on a second pass', () => {
    const host = document.createElement('div');
    host.innerHTML = pluginMountHtml('nosuch/thing');
    const a = mountPluginBlocks(host);
    const first = host.querySelectorAll('[data-testid]').length;
    const b = mountPluginBlocks(host);
    expect(host.querySelectorAll('[data-testid]').length).toBe(first);
    a();
    b();
  });

  it('ignores a placeholder with no plugin name', () => {
    const host = document.createElement('div');
    host.innerHTML = '<div class="plugin-mount"></div>';
    const dispose = mountPluginBlocks(host);
    expect(host.querySelectorAll('[data-testid]').length).toBe(0);
    dispose();
  });
});

describe('the block registry', () => {
  it('resolves a registered block and misses an unregistered one', async () => {
    // register-blocks runs on import of PluginBlock; import it for the side effect.
    await import('../PluginBlock');
    expect(lookupBlock('kanban', 'board')).toBeDefined();
    expect(lookupBlock('kanban', 'nope')).toBeUndefined();
  });

  it('keeps registration keyed per plugin and block', () => {
    const Stub = () => null;
    registerBlock('demo', 'one', Stub);
    expect(lookupBlock('demo', 'one')).toBe(Stub);
    expect(lookupBlock('demo', 'two')).toBeUndefined();
  });
});

// The fallback is the thing that stops the ecosystem splitting by surface: a
// plugin that publishes data and ships no component must still be visible.
describe('GenericBlock', () => {
  it('renders published rows as a table without any plugin-supplied layout', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            publications: {
              'demo:list': { demo: [{ name: 'alpha', status: 'todo' }] },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    const { container } = render(() => (
      <GenericBlock plugin="demo" block="list" params={{}} />
    ));
    await waitFor(() => expect(container.textContent).toContain('alpha'));
    expect(container.querySelector('table')).toBeTruthy();
    expect(container.textContent).toContain('status');
  });

  it('says so, rather than rendering nothing, when the plugin published nothing', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ publications: {} }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    const { container } = render(() => (
      <GenericBlock plugin="demo" block="list" params={{}} />
    ));
    await waitFor(() => expect(container.textContent).toContain('has published nothing'));
  });
});

// Drag-and-drop is the capability that moved the seam: an Oil tree could never
// express it, because a terminal has no gesture to project. So it is worth a
// test at the tier that runs on every commit.
describe('KanbanBlock', () => {
  const board = {
    columns: ['todo', 'doing', 'done'],
    tickets: [{ file: 'alpha.md', title: 'Alpha', status: 'todo' }],
    folder: 'tickets',
  };

  function stubFetch(
    onCommand?: (body: unknown) => void,
    onCaller?: (url: string, caller: string | undefined) => void,
  ) {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string, init?: RequestInit) => {
        onCaller?.(
          String(url),
          (init?.headers as Record<string, string> | undefined)?.['X-Crucible-Plugin'],
        );
        if (String(url).includes('/api/plugins/command')) {
          onCommand?.(JSON.parse(String(init?.body ?? '{}')));
          return new Response(JSON.stringify({ result: { ok: true } }), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          });
        }
        return new Response(JSON.stringify({ publications: { 'kanban:board': { kanban: board } } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
  }

  it('groups the published tickets into the published column order', async () => {
    stubFetch();
    const { container } = render(() => (
      <KanbanBlock plugin="kanban" block="board" params={{ folder: 'tickets' }} />
    ));
    await waitFor(() => expect(container.textContent).toContain('Alpha'));
    expect(container.textContent).toContain('todo (1)');
    expect(container.textContent).toContain('doing (0)');
  });

  it('sends a move command naming the ticket and the column it was dropped on', async () => {
    let sent: any;
    stubFetch((body) => {
      sent = body;
    });
    const { container } = render(() => (
      <KanbanBlock plugin="kanban" block="board" params={{ folder: 'tickets', kiln: 'k' }} />
    ));
    await waitFor(() => expect(container.textContent).toContain('Alpha'));

    const card = container.querySelector('[draggable="true"]')!;
    const columns = container.querySelectorAll('.flex-wrap > div');
    fireEvent.dragStart(card);
    fireEvent.drop(columns[1]);

    await waitFor(() => expect(sent).toBeTruthy());
    expect(sent.name).toBe('kanban_move');
    expect(sent.args).toMatchObject({ file: 'alpha.md', to: 'doing', folder: 'tickets', kiln: 'k' });
  });

  // The block declares which plugin it draws for, on the read AND on the
  // write. Without it the block is indistinguishable from the app and the
  // route's per-plugin comparison never runs in production — the Rust tests
  // would still pass, because they send the header by hand.
  //
  // Asserted, not proved: `props.plugin` is the fence's first line, so a note
  // author chose it. See `routes/plugin_caller.rs`.
  it('declares itself as the plugin it draws for on every call it makes', async () => {
    const callers = new Map<string, string | undefined>();
    stubFetch(undefined, (url, caller) => {
      callers.set(url.includes('/command') ? 'command' : 'publications', caller);
    });
    const { container } = render(() => (
      <KanbanBlock plugin="kanban" block="board" params={{ folder: 'tickets' }} />
    ));
    await waitFor(() => expect(container.textContent).toContain('Alpha'));
    expect(callers.get('publications')).toBe('kanban');

    const card = container.querySelector('[draggable="true"]')!;
    const columns = container.querySelectorAll('.flex-wrap > div');
    fireEvent.dragStart(card);
    fireEvent.drop(columns[1]);

    await waitFor(() => expect(callers.get('command')).toBe('kanban'));
  });

  // The plugin republishes and the push re-renders. A component that applied
  // the move locally would hold a second description of the board, free to
  // disagree with the plugin's.
  it('does not move the card locally before the plugin answers', async () => {
    stubFetch();
    const { container } = render(() => (
      <KanbanBlock plugin="kanban" block="board" params={{}} />
    ));
    await waitFor(() => expect(container.textContent).toContain('Alpha'));
    const card = container.querySelector('[draggable="true"]')!;
    fireEvent.dragStart(card);
    fireEvent.drop(container.querySelectorAll('.flex-wrap > div')[1]);
    // Still in todo: the stubbed publication never changed.
    expect(container.textContent).toContain('todo (1)');
  });
});

// A plugin could contribute content to a document and could not contribute a
// panel, which blocked rebuilding any existing panel as a plugin. These pin
// the seam that unblocked it.
describe('PluginBlockPanel', () => {
  it('lists every published plugin/key pair', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            publications: {
              'kanban:board': { kanban: {} },
              'demo:list': { demo: {} },
            },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    const { PluginBlockPanel } = await import('../PluginBlockPanel');
    const { container } = render(() => <PluginBlockPanel />);
    await waitFor(() => expect(container.textContent).toContain('kanban'));
    expect(container.textContent).toContain('board');
    expect(container.textContent).toContain('demo');
  });

  /**
   * `getPluginCommands` had zero callers: the daemon knew every executable
   * primitive a plugin declared and no surface offered one. This is the gate
   * on the consumer — the panel lists them, and pressing one opens the
   * GENERATED dialog rather than a hand-written form.
   */
  it('offers each declared command, with the effect the plugin declared', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const body = url.includes('/api/plugins/commands')
          ? {
              commands: [
                {
                  plugin: 'kanban',
                  name: 'kanban_move',
                  description: 'Move a ticket',
                  effect: 'write',
                  parameters: {
                    type: 'object',
                    properties: { file: { type: 'string', description: 'Ticket file' } },
                    required: ['file'],
                  },
                },
                { plugin: 'graph', name: 'graph_neighborhood', effect: 'read' },
              ],
            }
          : { publications: {} };
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    const { PluginBlockPanel } = await import('../PluginBlockPanel');
    const { container, getByText } = render(() => <PluginBlockPanel />);

    await waitFor(() => expect(container.textContent).toContain('kanban_move'));
    expect(container.textContent).toContain('graph_neighborhood');
    expect(container.textContent).toContain('write');
    expect(container.textContent).toContain('read');

    fireEvent.click(getByText('kanban_move'));
    await waitFor(() =>
      expect(container.querySelector('#plugin-command-field-file')).toBeTruthy(),
    );
  });

  it('says so when nothing has been published, rather than rendering blank', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(JSON.stringify({ publications: {} }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    const { PluginBlockPanel } = await import('../PluginBlockPanel');
    const { container } = render(() => <PluginBlockPanel />);
    await waitFor(() => expect(container.textContent).toContain('No plugin has published'));
  });
});
