import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetConfigForTests } from '@/lib/query/config';
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

/**
 * The pane reads and writes through the shared config query, so the real
 * `getConfig` and `saveConfig` run against this fetch. The saved body is kept
 * to be asserted: what the daemon receives is the thing under test.
 */
let env: TestQueryEnv;
let saved: unknown[] = [];

const openFileAtLineMock = vi.fn();
vi.mock('@/lib/file-actions', () => ({
  openFileAtLine: (...args: unknown[]) => openFileAtLineMock(...args),
}));

vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: vi.fn() },
}));

beforeEach(() => {
  localStorage.clear();
  resetConfigForTests();
  saved = [];
  env = createTestQueryEnv({
    'GET /api/config': () => ANSWER,
    'POST /api/config': async (request) => {
      saved.push(await request.json());
      return { ok: true, refused: [], rejected: [] };
    },
  });
});

afterEach(() => {
  cleanup();
  env.restore();
  resetConfigForTests();
  localStorage.clear();
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
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({ values: { chat: { endpoint: 'http://elsewhere:11434' } } });

    // The save invalidates the read, so the pane ends up showing what the
    // daemon holds rather than what was typed at it.
    await waitFor(() => expect(env.fetch.calls('GET /api/config')).toBe(2));
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
