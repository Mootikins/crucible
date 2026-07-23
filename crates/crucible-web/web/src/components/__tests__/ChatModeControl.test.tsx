import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

const mockSwitchMode = vi.fn();
let currentMode = 'normal';

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    chatMode: () => currentMode,
    switchMode: mockSwitchMode,
  }),
}));

import { ChatModeControl, nextChatMode } from '../ChatModeControl';

beforeEach(() => {
  vi.clearAllMocks();
  currentMode = 'normal';
});

// The control is now the launchpad's ChipSelect dropdown (popout renders
// through a Portal into document.body — query via screen).
describe('ChatModeControl', () => {
  it('picking a mode from the dropdown persists it via switchMode', () => {
    render(() => <ChatModeControl />);

    fireEvent.click(screen.getByTestId('chat-mode'));
    fireEvent.click(screen.getByTestId('mode-plan'));
    expect(mockSwitchMode).toHaveBeenCalledWith('plan');

    fireEvent.click(screen.getByTestId('chat-mode'));
    fireEvent.click(screen.getByTestId('mode-auto'));
    expect(mockSwitchMode).toHaveBeenCalledWith('auto');
  });

  it('shows the current mode on the trigger and checks it in the list', () => {
    currentMode = 'plan';
    render(() => <ChatModeControl />);
    expect(screen.getByTestId('chat-mode').textContent).toContain('Plan');

    fireEvent.click(screen.getByTestId('chat-mode'));
    expect(screen.getByTestId('mode-plan').getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('mode-normal').getAttribute('aria-selected')).toBe('false');
  });
});

describe('nextChatMode', () => {
  it('cycles normal → plan → auto → normal', () => {
    expect(nextChatMode('normal')).toBe('plan');
    expect(nextChatMode('plan')).toBe('auto');
    expect(nextChatMode('auto')).toBe('normal');
  });
});
