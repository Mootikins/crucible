import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { pluginMountHtml } from '@/lib/markdown';
import { mountPluginBlocks } from '../mount';
import { GenericBlock } from '../GenericBlock';
import { KanbanBlock } from '../KanbanBlock';
import { lookupBlock, registerBlock } from '../registry';
import { createTestQueryClient } from '@/test-utils/query';
import { setQueryClientForTests } from '@/lib/query/client';
import { baseOptions } from '@/test-utils/bases';

// EventSource does not exist in jsdom, and usePublication opens one. A stub is
// enough: these tests exercise the first read and the render, not the push.
class StubEventSource {
  addEventListener() {}
  close() {}
}
beforeEach(() => {
  vi.stubGlobal('EventSource', StubEventSource);
  // A fresh cache per case. A block's value is a cache entry now, so one case's
  // publication would answer the next case's block without asking its stub.
  setQueryClientForTests(createTestQueryClient());
});

afterEach(() => {
  setQueryClientForTests(null);
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

describe('KanbanBlock', () => {
  /** Answers the kiln roster and each base query; `seen` holds each query URL. */
  function daemon(seen: URL[]) {
    vi.stubGlobal('fetch', vi.fn(async (request: Request) => {
      if (request.url.includes('/api/kilns')) return Response.json({ kilns: [{name: 'Work', path: '/kiln', registered:true}], default_kiln:'Work' });
      if (request.url.includes('/api/bases/query')) {
        seen.push(new URL(request.url));
        return Response.json({root:'/kiln',view:'Board',view_type:'kanban',columns:[],rows:[],groups:[],summaries:{},options:baseOptions(),views:[{name:'Board',type:'kanban'}]});
      }
      throw new Error(`Unexpected legacy publication request: ${request.url}`);
    }));
  }

  it('routes a legacy embed with no kiln to the ticket base of the note that shows it', async () => {
    const seen: URL[] = [];
    daemon(seen);
    const {container} = render(() => <KanbanBlock plugin="kanban" block="board" params={{}} host="/kiln/notes/Plan.md" />);
    await waitFor(() => expect(container.querySelector('.base-view select')).toBeTruthy());
    expect(seen[0]!.searchParams.get('kiln')).toBe('Work');
    expect(seen[0]!.searchParams.get('path')).toBe('tickets.base');
    // No view is named, so the daemon answers the first view of the base.
    expect(seen[0]!.searchParams.has('view')).toBe(false);
    expect(seen[0]!.searchParams.get('this')).toBe('/kiln/notes/Plan.md');
  });

  it('honors the base, view and kiln of the block', async () => {
    const seen: URL[] = [];
    daemon(seen);
    const {container} = render(() => <KanbanBlock plugin="kanban" block="board" params={{kiln:'Work', base:'work.base', view:'Sprint'}} />);
    await waitFor(() => expect(container.querySelector('.base-view select')).toBeTruthy());
    expect(seen[0]!.searchParams.get('path')).toBe('work.base');
    expect(seen[0]!.searchParams.get('view')).toBe('Sprint');
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
      vi.fn(async (request: Request) => {
        const url = request.url;
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
