import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@solidjs/testing-library';
import { AppConfigSettingsSection } from '../AppConfigSettings';

/**
 * The daemon's answer, with one pinned leaf and one free one beside it.
 *
 * `pinned` comes from the daemon, which owns the refusal rule; this file never
 * derives a lock from the source word.
 */
const ANSWER = {
  kiln_path: '/kilns/main',
  config: { chat: { model: 'held-by-init-lua', endpoint: 'http://localhost:11434' } },
  config_root: '/home/u/.config/crucible',
  origins: [
    {
      key: 'chat.model',
      value: 'held-by-init-lua',
      source: 'lua',
      file: '/home/u/.config/crucible/init.lua',
      line: 12,
      pinned: true,
    },
    { key: 'chat.endpoint', value: 'http://localhost:11434', source: 'settings', pinned: false },
  ],
  controls: {
    options: {
      type: 'group',
      name: 'Crucible',
      args: [
        {
          key: 'chat',
          path: 'chat',
          type: 'group',
          name: 'Chat',
          order: 10,
          args: [
            {
              key: 'model',
              path: 'chat.model',
              type: 'input',
              name: 'Model',
              desc: 'Default model for a new session.',
              order: 1,
              default: '',
              writable: true,
            },
            {
              key: 'endpoint',
              path: 'chat.endpoint',
              type: 'input',
              name: 'Endpoint',
              order: 2,
              default: '',
              writable: true,
            },
          ],
        },
      ],
    },
    read_only: [
      {
        path: 'data_home',
        reason: 'A location key names WHERE the daemon acts.',
      },
    ],
  },
};

const getConfigMock = vi.fn(async () => ANSWER);
const saveConfigMock = vi.fn(async (_values: Record<string, unknown>) => ({
  ok: true,
  refused: [] as unknown[],
  rejected: [] as string[],
}));
vi.mock('@/lib/api', () => ({
  getConfig: () => getConfigMock(),
  saveConfig: (values: Record<string, unknown>) => saveConfigMock(values),
}));

const openFileAtLineMock = vi.fn();
vi.mock('@/lib/file-actions', () => ({
  openFileAtLine: (...args: unknown[]) => openFileAtLineMock(...args),
}));

vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: vi.fn() },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/** The section renders rows, so the caller owns the table. */
function renderSection(onClose?: () => void) {
  return render(() => (
    <table>
      <tbody>
        <AppConfigSettingsSection onClose={onClose} />
      </tbody>
    </table>
  ));
}

describe('the app-config settings pane', () => {
  it('renders a key the daemon reports as pinned as a disabled control', async () => {
    renderSection();

    const row = await waitFor(() => screen.getByTestId('config-option-chat.model'));
    const control = row.querySelector('input') as HTMLInputElement;
    expect(control.value).toBe('held-by-init-lua');
    // A save would be refused, so the control must not offer one.
    expect(control.disabled, 'a pinned control is disabled').toBe(true);
  });

  it('names the file and the line that hold a pinned key', async () => {
    renderSection();

    const row = await waitFor(() => screen.getByTestId('config-option-chat.model'));
    // The route out of the lock: which file, which line.
    expect(row.textContent).toContain('init.lua');
    expect(row.textContent).toContain('12');
    // And it never claims the user asked for this value on every machine: the
    // line may sit inside a test on the hostname.
    expect(row.textContent).toContain('conditional on this host');
  });

  it('offers a jump to the line that holds the key, and clears the dialog first', async () => {
    const onClose = vi.fn();
    renderSection(onClose);

    const row = await waitFor(() => screen.getByTestId('config-option-chat.model'));
    fireEvent.click(row.querySelector('[data-testid="config-jump-to-pin"]') as HTMLElement);

    expect(openFileAtLineMock).toHaveBeenCalledWith(
      '/home/u/.config/crucible/init.lua',
      12,
      'init.lua',
    );
    // The editor opens behind the modal, so the modal has to go.
    expect(onClose).toHaveBeenCalled();
  });

  it('leaves an unpinned key writable, and saves it by its config path', async () => {
    renderSection();

    const row = await waitFor(() => screen.getByTestId('config-option-chat.endpoint'));
    const control = row.querySelector('input') as HTMLInputElement;
    expect(control.disabled, 'nothing pins this leaf').toBe(false);

    fireEvent.change(control, { target: { value: 'http://elsewhere:11434' } });
    await waitFor(() => expect(saveConfigMock).toHaveBeenCalled());
    expect(saveConfigMock).toHaveBeenCalledWith({
      chat: { endpoint: 'http://elsewhere:11434' },
    });
  });

  it('shows a leaf that takes no control, with the reason it takes none', async () => {
    renderSection();

    // A user hunting for data_home learns why it is not editable here, rather
    // than concluding the settings are incomplete.
    const row = await waitFor(() => screen.getByTestId('config-readonly-data_home'));
    expect(row.textContent).toContain('A location key names WHERE the daemon acts.');
    expect(row.querySelector('input')).toBeNull();
  });
});
