import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

const getPluginsMock = vi.fn();
const reloadPluginMock = vi.fn();
const installPluginMock = vi.fn();
const removePluginMock = vi.fn();

vi.mock('@/lib/api', () => ({
  getPlugins: () => getPluginsMock(),
  reloadPlugin: (...args: unknown[]) => reloadPluginMock(...args),
  installPlugin: (...args: unknown[]) => installPluginMock(...args),
  removePlugin: (...args: unknown[]) => removePluginMock(...args),
}));

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

describe('PluginPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getPluginsMock.mockResolvedValue([RICH_ROW]);
    reloadPluginMock.mockResolvedValue({
      name: 'demo-plugin',
      reloaded: true,
      tools: 3,
      commands: 1,
      handlers: 2,
      services: 0,
    });
  });

  it('renders rows from the rich plugin_info response', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(getPluginsMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByTestId('plugin-row-demo-plugin')).toBeInTheDocument());

    // Source + state badges visible.
    expect(screen.getByText('User')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();
    expect(screen.getByText(/3T 1C 2H 0S/)).toBeInTheDocument();
  });

  it('reload button triggers reloadPlugin and shows a success toast', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-reload-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-reload-demo-plugin'));
    await waitFor(() => expect(reloadPluginMock).toHaveBeenCalledWith('demo-plugin'));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'success',
        expect.stringContaining('Reloaded demo-plugin'),
      ),
    );
    // Refreshes the list.
    expect(getPluginsMock.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('renders empty state when no plugins discovered', async () => {
    getPluginsMock.mockResolvedValue([]);
    render(() => <PluginPanel />);
    await waitFor(() =>
      expect(screen.getByText(/No plugins discovered/i)).toBeInTheDocument(),
    );
  });

  it('shows error toast when listing fails', async () => {
    getPluginsMock.mockRejectedValueOnce(new Error('5xx'));
    render(() => <PluginPanel />);
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('5xx'),
      ),
    );
  });

  it('shows error toast when reload fails', async () => {
    reloadPluginMock.mockRejectedValueOnce(new Error('boom'));
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-reload-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-reload-demo-plugin'));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('boom'),
      ),
    );
  });

  it('install modal calls installPlugin with the entered URL', async () => {
    installPluginMock.mockResolvedValue({
      name: 'new-plugin',
      outcome: { kind: 'cloned', dest: '/tmp/new-plugin' },
      plugins_toml: '/tmp/plugins.toml',
    });
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugins-install-open')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugins-install-open'));
    expect(screen.getByTestId('plugins-install-modal')).toBeInTheDocument();

    fireEvent.input(screen.getByTestId('plugins-install-url'), {
      target: { value: 'user/repo' },
    });
    fireEvent.click(screen.getByTestId('plugins-install-submit'));

    await waitFor(() => expect(installPluginMock).toHaveBeenCalledWith({ url: 'user/repo' }));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'success',
        expect.stringContaining('Installed new-plugin'),
      ),
    );
    // Modal closes; refetch fires (initial + post-install).
    expect(getPluginsMock.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('install rejects obvious invalid URLs without calling the API', async () => {
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugins-install-open')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugins-install-open'));
    fireEvent.input(screen.getByTestId('plugins-install-url'), {
      target: { value: 'not a url' },
    });
    fireEvent.click(screen.getByTestId('plugins-install-submit'));

    expect(installPluginMock).not.toHaveBeenCalled();
    expect(addNotificationMock).toHaveBeenCalledWith(
      'error',
      expect.stringContaining('Invalid URL'),
    );
  });

  it('uninstall confirmation passes purge flag through to removePlugin', async () => {
    removePluginMock.mockResolvedValue({
      name: 'demo-plugin',
      plugins_toml: '/tmp/plugins.toml',
      purged_dir: '/tmp/demo',
    });
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-remove-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-remove-demo-plugin'));
    expect(screen.getByTestId('plugins-remove-modal')).toBeInTheDocument();

    // Check the purge checkbox.
    const purgeCheckbox = screen.getByTestId('plugins-remove-purge') as HTMLInputElement;
    fireEvent.click(purgeCheckbox);

    fireEvent.click(screen.getByTestId('plugins-remove-confirm'));
    await waitFor(() => expect(removePluginMock).toHaveBeenCalledWith('demo-plugin', true));
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'success',
        expect.stringContaining('Removed demo-plugin'),
      ),
    );
  });

  it('uninstall confirmation defaults to purge=false', async () => {
    removePluginMock.mockResolvedValue({
      name: 'demo-plugin',
      plugins_toml: '/tmp/plugins.toml',
      purged_dir: null,
    });
    render(() => <PluginPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-remove-demo-plugin')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('plugin-remove-demo-plugin'));
    fireEvent.click(screen.getByTestId('plugins-remove-confirm'));
    await waitFor(() => expect(removePluginMock).toHaveBeenCalledWith('demo-plugin', false));
  });
});
