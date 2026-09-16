import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/windowing/components/WindowManager', () => ({
  WindowManager: () => <div data-testid="window-manager" />,
}));
vi.mock('@/components/mobile/MobileShell', () => ({
  MobileShell: () => <div data-testid="mobile-shell" />,
}));

import { AppShell } from '@/components/AppShell';

beforeEach(() => {
  device.compact = false;
});

describe('AppShell', () => {
  it('draws the window manager on a desktop viewport', () => {
    render(() => <AppShell />);
    expect(screen.queryByTestId('window-manager')).toBeTruthy();
    expect(screen.queryByTestId('mobile-shell')).toBeNull();
  });

  it('draws the compact shell, and only it, on a phone viewport', () => {
    device.compact = true;
    render(() => <AppShell />);
    expect(screen.queryByTestId('mobile-shell')).toBeTruthy();
    expect(screen.queryByTestId('window-manager')).toBeNull();
  });
});
