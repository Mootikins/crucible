import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/lib/api', () => ({ getPluginOptions: () => Promise.resolve({}) }));
// Each section draws a form over every context; the layout is what is tested.
vi.mock('../sections', () => ({
  settingsSections: () => [{ id: 'editor', title: 'Editor', component: () => <div>editor form</div> }],
  settingsGroups: (sections: unknown[]) => [{ group: 'App', sections }],
}));

import { SettingsModal } from '@/components/settings/SettingsModal';

beforeEach(() => {
  device.compact = false;
});

describe('SettingsModal layout', () => {
  // A phone cannot hold the two-column dialog: the section list alone is
  // 216 px of a 412 px screen.
  it('takes the whole screen on a phone, with the sections as a strip', () => {
    device.compact = true;
    render(() => <SettingsModal open onClose={() => {}} />);
    const dialog = screen.getByTestId('settings-modal');
    expect(dialog.className).toContain('h-dvh');
    expect(dialog.className).not.toContain('grid-cols-');
    expect(dialog.querySelector('nav')!.className).toContain('flex-row');
  });

  it('keeps the two-column dialog on a desktop', () => {
    render(() => <SettingsModal open onClose={() => {}} />);
    const dialog = screen.getByTestId('settings-modal');
    expect(dialog.className).toContain('grid-cols-');
    expect(dialog.className).not.toContain('h-dvh');
    expect(dialog.querySelector('nav')!.className).toContain('flex-col');
  });
});
