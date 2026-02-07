import { render } from '@solidjs/testing-library';
import { describe, it, expect } from 'vitest';

import App from './App';

describe('App', () => {
  it('renders the layout container', () => {
    const { container } = render(() => <App />);
    const layoutContainer = container.querySelector('.h-full.w-full');
    expect(layoutContainer).toBeInTheDocument();
  });
});
