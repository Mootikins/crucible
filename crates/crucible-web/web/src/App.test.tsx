import { render, screen } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';

// Mock dockview-core to avoid DOM dependencies in unit tests
vi.mock('dockview-core', () => ({
  DockviewComponent: vi.fn(),
}));

import App from './App';

describe('App', () => {
  it('renders the shell layout with zone toggles', () => {
    render(() => <App />);
    expect(screen.getByTestId('toggle-left')).toBeInTheDocument();
    expect(screen.getByTestId('toggle-right')).toBeInTheDocument();
    expect(screen.getByTestId('toggle-bottom')).toBeInTheDocument();
  });

  it('renders four zone containers', () => {
    render(() => <App />);
    const zones = document.querySelectorAll('[data-zone]');
    expect(zones.length).toBe(4);
  });
});
