import { render, screen } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';

vi.mock('solid-dockview', () => ({
  DockView: (props: { children: any }) => <div data-testid="dock-view">{props.children}</div>,
  DockPanel: (props: { children: any; title: string }) => (
    <div data-testid={`dock-panel-${props.title?.toLowerCase()}`}>{props.children}</div>
  ),
}));

import App from './App';

describe('App', () => {
  it('renders the chat interface', () => {
    render(() => <App />);
    expect(screen.getByTestId('dock-view')).toBeInTheDocument();
    expect(screen.getByTestId('message-list')).toBeInTheDocument();
    expect(screen.getByTestId('chat-input-form')).toBeInTheDocument();
    expect(screen.getByTestId('mic-button')).toBeInTheDocument();
  });

  it('shows empty state message when no session', () => {
    render(() => <App />);
    expect(screen.getByText(/Select or create a session/)).toBeInTheDocument();
  });
});
