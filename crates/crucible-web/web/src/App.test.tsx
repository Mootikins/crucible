import { render, screen } from '@solidjs/testing-library';
import { describe, it, expect } from 'vitest';
import App from './App';

describe('App', () => {
  it('renders the chat interface', () => {
    render(() => <App />);
    expect(screen.getByTestId('message-list')).toBeInTheDocument();
    expect(screen.getByTestId('chat-input-form')).toBeInTheDocument();
    expect(screen.getByTestId('mic-button')).toBeInTheDocument();
  });

  it('shows empty state message', () => {
    render(() => <App />);
    expect(screen.getByText(/Start a conversation/)).toBeInTheDocument();
  });
});
