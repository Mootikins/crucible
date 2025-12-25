import { render, screen } from '@solidjs/testing-library';
import { describe, it, expect } from 'vitest';
import App from './App';

describe('App', () => {
  it('renders the title', () => {
    render(() => <App />);
    expect(screen.getByText('Crucible')).toBeInTheDocument();
  });
});
