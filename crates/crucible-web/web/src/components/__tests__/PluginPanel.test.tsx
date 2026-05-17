import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

const getPluginsMock = vi.fn();
const reloadPluginMock = vi.fn();

vi.mock('@/lib/api', () => ({
  getPlugins: () => getPluginsMock(),
  reloadPlugin: (...args: unknown[]) => reloadPluginMock(...args),
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
});
