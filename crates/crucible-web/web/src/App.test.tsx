import { render } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';

// Mock dockview-core to avoid DOM dependencies in unit tests
vi.mock('dockview-core', () => ({
  DockviewComponent: vi.fn(),
}));

import App from './App';

describe('App', () => {
  it('renders the dock layout container', () => {
    const { container } = render(() => <App />);
    const dockContainer = container.querySelector('.h-full.w-full');
    expect(dockContainer).toBeInTheDocument();
  });
});
