import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Palette, Pencil } from '@/lib/icons';

const device = vi.hoisted(() => ({ compact: false }));
const mounts = vi.hoisted(() => ({ deep: 0 }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/lib/api', () => ({ getPluginOptions: () => Promise.resolve({}) }));
// Each section draws a form over every context; the navigation is what is
// tested. The shape matches `SettingsSection` — a mock that drifts from it
// tests a screen the app never renders.
vi.mock('../sections', async () => {
  const actual = await vi.importActual<typeof import('../sections')>('../sections');
  return {
    ...actual,
    settingsSections: () => [
      { id: 'appearance', label: 'Appearance', icon: Palette, group: 'Look and feel', render: () => <tr><td>appearance form</td></tr> },
      { id: 'editor', label: 'Editor', icon: Pencil, group: 'Look and feel', render: () => <tr><td>editor form</td></tr> },
      { id: 'offline', label: 'Offline', icon: Palette, group: 'Workspace', render: () => <tr><td>offline form</td></tr> },
      {
        id: 'deep',
        label: 'Deep',
        icon: Palette,
        group: 'Workspace',
        render: () => {
          mounts.deep += 1;
          const nav = useSettingsStack();
          return (
            <tr>
              <td>
                <button
                  data-testid="drill"
                  onClick={() =>
                    nav?.push({ id: 'sub', title: 'Sub', rows: true, body: () => <tr><td>sub page</td></tr> })
                  }
                >
                  go deeper
                </button>
              </td>
            </tr>
          );
        },
      },
    ],
  };
});

import { SettingsModal } from '@/components/settings/SettingsModal';
import { useSettingsStack } from '@/components/settings/settings-nav';

beforeEach(() => {
  device.compact = false;
  mounts.deep = 0;
});

describe('SettingsModal layout', () => {
  it('keeps the two-column dialog on a desktop', () => {
    render(() => <SettingsModal open onClose={() => {}} />);
    const dialog = screen.getByTestId('settings-modal');
    expect(dialog.className).toContain('grid-cols-');
    expect(dialog.className).not.toContain('h-dvh');
    expect(dialog.querySelector('nav')!.className).toContain('flex-col');
  });

  // A phone cannot hold the two-column dialog: the section list alone is
  // 216 px of a 412 px screen.
  it('takes the whole screen on a phone, and drops the desktop section list', () => {
    device.compact = true;
    render(() => <SettingsModal open onClose={() => {}} />);
    const dialog = screen.getByTestId('settings-modal');
    expect(dialog.className).toContain('h-dvh');
    expect(dialog.className).not.toContain('grid-cols-');
    expect(dialog.querySelector('nav')).toBeNull();
  });
});

describe('SettingsModal on a phone navigates by drilling in', () => {
  beforeEach(() => {
    device.compact = true;
  });

  it('opens on a list of every category, under its group', () => {
    render(() => <SettingsModal open onClose={() => {}} />);

    expect(screen.getByText('Look and feel')).toBeTruthy();
    expect(screen.getByText('Workspace')).toBeTruthy();
    expect(screen.getByTestId('settings-nav-appearance')).toBeTruthy();
    expect(screen.getByTestId('settings-nav-offline')).toBeTruthy();
    // No form is shown until one is chosen — that is what "one list" means.
    expect(screen.queryByText('appearance form')).toBeNull();
    // Nothing to go back to at the root.
    expect(screen.queryByTestId('settings-back')).toBeNull();
  });

  it('opens the category that was tapped, and names it in the bar', () => {
    render(() => <SettingsModal open onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('settings-nav-editor'));

    expect(screen.getByText('editor form')).toBeTruthy();
    // The list it came from is HIDDEN, not scrolled past — and not unmounted
    // either, because a level that owns data must survive being drilled into.
    expect(screen.getByTestId('settings-nav-appearance').closest('.hidden')).not.toBeNull();
    expect(screen.getByTestId('settings-back')).toBeTruthy();
  });

  it('returns to the list when back is pressed', () => {
    render(() => <SettingsModal open onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('settings-nav-editor'));
    fireEvent.click(screen.getByTestId('settings-back'));

    expect(screen.getByTestId('settings-nav-appearance').closest('.hidden')).toBeNull();
    expect(screen.queryByText('editor form')).toBeNull();
    expect(screen.queryByTestId('settings-back')).toBeNull();
  });

  it('closes the dialog rather than a level when close is pressed deep in', () => {
    const onClose = vi.fn();
    render(() => <SettingsModal open onClose={onClose} />);
    fireEvent.click(screen.getByTestId('settings-nav-editor'));
    fireEvent.click(screen.getByTestId('settings-modal-close'));

    expect(onClose).toHaveBeenCalled();
  });

  // Escape at depth means "up one level". Closing the whole dialog from the
  // first keystroke would lose the user's place for no reason.
  it('walks up one level on Escape, and closes only at the root', () => {
    const onClose = vi.fn();
    render(() => <SettingsModal open onClose={onClose} />);
    fireEvent.click(screen.getByTestId('settings-nav-editor'));

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('settings-nav-appearance').closest('.hidden')).toBeNull();

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  /**
   * A section owns its data. Rendering only the top page disposed it the
   * moment the user drilled into one of its groups, so its inline save error
   * had no reader left and every Back refetched the whole tree.
   */
  it('keeps a section mounted while the user is inside one of its pages', async () => {
    render(() => <SettingsModal open onClose={() => {}} />);
    fireEvent.click(screen.getByTestId('settings-nav-deep'));
    expect(mounts.deep).toBe(1);

    fireEvent.click(screen.getByTestId('drill'));
    expect(screen.getByText('sub page')).toBeTruthy();
    expect(mounts.deep, 'drilling in must not dispose the section').toBe(1);

    fireEvent.click(screen.getByTestId('settings-back'));
    expect(mounts.deep, 'coming back must not remount and refetch').toBe(1);
  });
});
