import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen } from '@solidjs/testing-library';

/**
 * The content pane is the ONE scroll container of the desktop dialog.
 *
 * Regression with real history. The dialog is a fixed-height grid, and the
 * right column is a flex column whose pane is `min-h-0 flex-1
 * overflow-y-auto`. A grid item keeps `min-height: auto` unless told
 * otherwise, so on a section taller than the dialog (Editor, Plugins,
 * Configuration) the column grew to its content, the pane never overflowed,
 * and the dialog's own `overflow-hidden` cut the bottom of the form off with
 * no scrollbar anywhere. Short sections looked fine, which is why the report
 * said "some pages".
 *
 * jsdom does not lay out, so this pins the classes that decide the layout:
 * the column must be `min-h-0`, the pane must keep its three classes, and no
 * section may bring a second scroll container or a full-height box.
 */
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => false }));
// No `vi.mock('@/lib/api')`: every read the sections make on render — the
// daemon config, the plugin settings trees, the plugin roster, one option's
// current value — arrives over the ROUTES in `beforeEach`. The writes the
// sections can issue stay unnamed: nothing here presses them, and an unnamed
// route answers 404 the way a test that wanted one would see.

// The offline section reads IndexedDB, which jsdom does not have.
vi.mock('@/lib/offline/sync', () => ({
  cacheKiln: async () => ({ notes: 0, attachments: 0, failed: [] }),
  dropKiln: async () => {},
  kilnSize: async () => ({ notes: 0, attachments: 0 }),
  syncNow: async () => ({ sent: 0, conflicted: [], foreign: 0, failed: 0 }),
  pendingCount: async () => 0,
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => null,
    sessions: () => [],
    refreshSessions: async () => {},
  }),
}));

import { SettingsModal } from '@/components/settings/SettingsModal';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { settingsSections } from '@/components/settings/sections';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

// The Offline section lists the roster, which arrives over the fetch.
let kilnEnv: TestQueryEnv;

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  kilnEnv = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: [] }),
    // `getMcpStatus` is NOT stubbed: the MCP section reads it through
    // `useMcpStatus`, which runs the real one against this route.
    'GET /api/mcp/status': () => ({ servers: [] }),
    'GET /api/config': () => ({
      kiln_path: '/kilns/main',
      config: {},
      config_root: '/home/u/.config/crucible',
      origins: [],
      controls: { options: { type: 'group', name: 'Crucible', args: [] } },
    }),
    // The one plugin section: its tree is what names `plugin:widget` above.
    'GET /api/plugins/options': () => ({
      options: {
        widget: {
          type: 'group',
          name: 'Widget',
          args: [{ key: 'shade', path: 'shade', type: 'input', name: 'Shade', default: '' }],
        },
      },
    }),
    // One route answers all three option actions; `get` carries the value.
    'POST /api/plugins/widget/option': () => ({ value: '', ok: true }),
    'GET /api/plugins': () => ({ plugins: [] }),
  });
});

afterEach(() => {
  kilnEnv.restore();
  resetKilnsForTests();
  cleanup();
});

/** A class that makes an element size or scroll itself. */
const SELF_SIZING = /(^|\s)(overflow-y-auto|overflow-auto|overflow-y-scroll|overflow-scroll|h-full|h-screen|h-dvh|sticky)(\s|$)/;

function pane(): HTMLElement {
  const dialog = screen.getByTestId('settings-modal');
  const found = dialog.querySelector('.px-5.pb-6') as HTMLElement | null;
  if (!found) throw new Error('the content pane is gone');
  return found;
}

// Every built-in section, and the one plugin section the mocked daemon declares.
const sectionIds = [...settingsSections().map((s) => s.id), 'plugin:widget'];

describe('the settings content pane is the one scroll container', () => {
  it.each(sectionIds)('on the %s section', async (id) => {
    render(() => (
      <SettingsProvider>
        <SettingsModal open onClose={() => {}} />
      </SettingsProvider>
    ));
    // A plugin section exists once the trees are fetched; wait for its row.
    const nav = await screen.findByTestId(`settings-nav-${id}`);
    fireEvent.click(nav);
    // Let the section's own fetches land so its full form is on screen.
    await new Promise((r) => setTimeout(r, 0));

    const content = pane();
    for (const cls of ['min-h-0', 'flex-1', 'overflow-y-auto']) {
      expect(content.classList.contains(cls), `pane keeps ${cls}`).toBe(true);
    }
    // The column the pane fills is a grid item. Without `min-h-0` it grows to
    // its content and the pane has nothing to overflow.
    const column = content.parentElement!;
    expect(column.classList.contains('min-h-0'), 'column is min-h-0').toBe(true);

    // The section's own markup: rows, and everything inside them. A form
    // control (a textarea) scrolls natively and is not a layout box.
    const offenders = [...content.querySelectorAll('*')]
      .filter((el) => !['TEXTAREA', 'INPUT', 'SELECT'].includes(el.tagName))
      .filter((el) => SELF_SIZING.test(el.className))
      .map((el) => `${el.tagName.toLowerCase()}.${el.className}`);
    expect(offenders, `${id} brings its own scroll or height`).toEqual([]);
  });
});
