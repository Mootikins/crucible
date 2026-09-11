import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

// The drawers' panels need every context; the shell's own job is the frame.
vi.mock('@/components/SessionsPanel', () => ({
  SessionsPanel: () => <div data-testid="sessions-panel" />,
}));
vi.mock('@/components/FilesPanel', () => ({
  FilesPanel: () => <div data-testid="files-panel" />,
}));

import { MobileShell } from '@/components/mobile/MobileShell';

const isOpen = (side: 'left' | 'right') =>
  !screen.getByTestId(`drawer-${side}`).hasAttribute('inert');

describe('MobileShell', () => {
  it('holds the sessions panel in the left drawer and files in the right', () => {
    render(() => <MobileShell />);
    expect(screen.getByTestId('drawer-left').contains(screen.getByTestId('sessions-panel'))).toBe(true);
    expect(screen.getByTestId('drawer-right').contains(screen.getByTestId('files-panel'))).toBe(true);
  });

  it('opens each drawer from its app-bar button', () => {
    render(() => <MobileShell />);
    expect(isOpen('left')).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Sessions' }));
    expect(isOpen('left')).toBe(true);
  });

  it('never shows both drawers at once', () => {
    render(() => <MobileShell />);
    fireEvent.click(screen.getByRole('button', { name: 'Sessions' }));
    fireEvent.click(screen.getByRole('button', { name: 'Files' }));
    expect(isOpen('right')).toBe(true);
    expect(isOpen('left')).toBe(false);
  });

  it('gives each app-bar button a touch-sized target', () => {
    render(() => <MobileShell />);
    for (const name of ['Sessions', 'Files']) {
      const cls = screen.getByRole('button', { name }).className;
      expect(cls).toMatch(/\bw-11\b/);
      expect(cls).toMatch(/\bh-11\b/);
    }
  });
});
