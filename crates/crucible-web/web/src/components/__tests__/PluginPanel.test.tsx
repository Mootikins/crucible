import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

const addNotificationMock = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...args: unknown[]) => addNotificationMock(...args) },
}));

import { PluginPanel } from '../PluginPanel';

const RICH_ROW = {
  name: 'demo-plugin',
  version: '1.2.3',
  source: 'User',
  state: 'Active',
  dir: '/tmp/demo',
  tools: 3,
  commands: 1,
  handlers: 2,
  services: 0,
};

/**
 * The panel drives the daemon's own routes now, not a stub of `lib/api`.
 *
 * That is the point of the change it covers: the roster, the option trees and
 * the three writes live on one cache, so "the install refreshed the list" is a
 * statement about a REQUEST the panel made, which a function-level stub could
 * not see.
 */
let env: TestQueryEnv;
let roster: unknown[];
let installed: { url?: string } | null;
let removed: string | null;
let reloaded: string | null;

function routes() {
  return {
    'GET /api/plugins': () => ({ plugins: roster }),
    'GET /api/plugins/options': () => ({ options: {} }),
    'GET /api/plugins/commands': () => ({ commands: [] }),
    'POST /api/plugins': async (request: Request) => {
      installed = (await request.json()) as { url?: string };
      return {
        name: 'new-plugin',
        outcome: { kind: 'cloned', dest: '/tmp/new-plugin' },
        plugins_toml: '/tmp/plugins.toml',
        installed: true,
        loaded: true,
        tools: 1,
        commands: 0,
        services: 0,
        error: null,
      };
    },
    'DELETE /api/plugins/demo-plugin': (request: Request) => {
      removed = new URL(request.url).search;
      return {
        name: 'demo-plugin',
        plugins_toml: '/tmp/plugins.toml',
        purged_dir: removed.includes('purge=true') ? '/tmp/demo' : null,
        purge_error: null,
        kept_dir: null,
      };
    },
    'POST /api/plugins/demo-plugin/reload': () => {
      reloaded = 'demo-plugin';
      return { name: 'demo-plugin', tools: 3, commands: 1, handlers: 2, services: 0 };
    },
  };
}

describe('PluginPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    roster = [RICH_ROW];
    installed = null;
    removed = null;
    reloaded = null;
    env = createTestQueryEnv(routes());
  });

  afterEach(() => {
    env?.restore();
  });

  it('renders rows from the rich plugin_info response', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(env.fetch.calls('GET /api/plugins')).toBe(1));
    await waitFor(() => expect(screen.getByTestId('plugin-row-demo-plugin')).toBeInTheDocument());

    // Source + state badges visible.
    expect(screen.getByText('User')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
    expect(screen.getByText(/3T 1C 2H 0S/)).toBeInTheDocument();
  });

  it('renders a plugin with no version without printing a null', async () => {
    // A plugin without a `spec.luau` fragment has no version: discovery reads
    // the version from the fragment, and nothing else declares one.
    // The daemon sends null. `v${null}` renders "vnull", which is worse than
    // the "0.0.0" placeholder it replaced, so the row names the state.
    roster = [{ ...RICH_ROW, name: 'unloaded-plugin', version: null }];
    render(() => <PluginPanel />);
    await waitFor(() =>
      expect(screen.getByTestId('plugin-row-unloaded-plugin')).toBeInTheDocument(),
    );

    const row = screen.getByTestId('plugin-row-unloaded-plugin');
    expect(row.textContent).not.toMatch(/null|undefined|None|0\.0\.0/);
    expect(screen.getByTestId('plugin-version-unloaded-plugin')).toHaveTextContent(
      'version unknown',
    );
  });

  it('shows last_error for a broken plugin, and no error row for a healthy one', async () => {
    roster = [
      RICH_ROW,
      {
        ...RICH_ROW,
        name: 'broken-plugin',
        state: 'Error',
        last_error: "init.lua:12: module 'lua.container' not found",
      },
    ];
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-broken-plugin')).toBeInTheDocument());

    // The reason, where the user is looking — "Error" alone is unactionable.
    expect(screen.getByTestId('plugin-error-broken-plugin')).toHaveTextContent(
      "module 'lua.container' not found",
    );
    expect(screen.queryByTestId('plugin-error-demo-plugin')).not.toBeInTheDocument();
  });

  it('reload button reloads the plugin and shows a success toast', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-reload-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-reload-demo-plugin'));
    await waitFor(() => expect(reloaded).toBe('demo-plugin'));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'success',
        expect.stringContaining('Reloaded demo-plugin'),
      ),
    );
    // Refreshes the roster AND the trees. The trees are what the old code got
    // wrong from the settings pane: a reloaded plugin's accessors close over
    // the previous load, so a held tree describes a version that is gone.
    await waitFor(() => expect(env.fetch.calls('GET /api/plugins')).toBe(2));
    expect(env.fetch.calls('GET /api/plugins/options')).toBe(2);
  });

  it('renders empty state when no plugins discovered', async () => {
    roster = [];
    render(() => <PluginPanel />);
    await waitFor(() =>
      expect(screen.getByText(/No plugins discovered/i)).toBeInTheDocument(),
    );
  });

  it('shows the refusal rather than an empty roster when listing fails', async () => {
    env.restore();
    env = createTestQueryEnv({
      ...routes(),
      'GET /api/plugins': apiError(500, 'the plugin host is unreachable'),
    });
    render(() => <PluginPanel />);

    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('Failed to list plugins'),
      ),
    );
    // "No plugins discovered" over a failed read states something false about
    // the user's install.
    expect(screen.getByTestId('plugins-error')).toBeInTheDocument();
    expect(screen.queryByText(/No plugins discovered/i)).not.toBeInTheDocument();
  });

  it('shows error toast when reload fails', async () => {
    env.restore();
    env = createTestQueryEnv({
      ...routes(),
      'POST /api/plugins/demo-plugin/reload': apiError(500, 'the module failed to load'),
    });
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-reload-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-reload-demo-plugin'));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('Failed to reload plugin'),
      ),
    );
  });

  it('install modal installs the entered URL and refreshes the roster', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugins-install-open')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugins-install-open'));
    expect(screen.getByTestId('plugins-install-modal')).toBeInTheDocument();

    fireEvent.input(screen.getByTestId('plugins-install-url'), {
      target: { value: 'user/repo' },
    });
    fireEvent.click(screen.getByTestId('plugins-install-submit'));

    await waitFor(() => expect(installed).toEqual({ url: 'user/repo' }));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'success',
        expect.stringContaining('Installed new-plugin'),
      ),
    );
    // The hazard this task exists to remove: an install that refreshed nothing.
    await waitFor(() => expect(env.fetch.calls('GET /api/plugins')).toBe(2));
    expect(env.fetch.calls('GET /api/plugins/options')).toBe(2);
  });

  it('install rejects obvious invalid URLs without calling the API', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugins-install-open')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugins-install-open'));
    fireEvent.input(screen.getByTestId('plugins-install-url'), {
      target: { value: 'not a url' },
    });
    fireEvent.click(screen.getByTestId('plugins-install-submit'));

    expect(env.fetch.calls('POST /api/plugins')).toBe(0);
    expect(addNotificationMock).toHaveBeenCalledWith(
      'error',
      expect.stringContaining('Invalid URL'),
    );
  });

  it('uninstall confirmation passes the purge flag through to the daemon', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-remove-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-remove-demo-plugin'));
    expect(screen.getByTestId('plugins-remove-modal')).toBeInTheDocument();

    // Check the purge checkbox.
    const purgeCheckbox = screen.getByTestId('plugins-remove-purge') as HTMLInputElement;
    fireEvent.click(purgeCheckbox);

    fireEvent.click(screen.getByTestId('plugins-remove-confirm'));
    await waitFor(() => expect(removed).toBe('?purge=true'));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'success',
        expect.stringContaining('Removed demo-plugin'),
      ),
    );
  });

  it('uninstall confirmation defaults to purge=false', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-remove-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-remove-demo-plugin'));
    fireEvent.click(screen.getByTestId('plugins-remove-confirm'));
    await waitFor(() => expect(removed).toBe(''));
  });
});
