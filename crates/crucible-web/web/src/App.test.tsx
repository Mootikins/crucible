import { render } from '@solidjs/testing-library';
import { describe, it, expect } from 'vitest';

import App from './App';

describe('App', () => {
  it('renders the window manager with header and main area', () => {
    const { container } = render(() => <App />);
    const header = container.querySelector('.bg-zinc-900.border-b');
    const main = container.querySelector('.bg-zinc-950');
    expect(header).toBeInTheDocument();
    expect(main).toBeInTheDocument();
  });
});
