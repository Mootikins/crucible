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

describe('ChatModeControl', () => {
  it('clicking a mode persists it via switchMode (not just local state)', () => {
    render(() => <ChatModeControl />);

    fireEvent.click(screen.getByTestId('mode-plan'));
    expect(mockSwitchMode).toHaveBeenCalledWith('plan');

    fireEvent.click(screen.getByTestId('mode-auto'));
    expect(mockSwitchMode).toHaveBeenCalledWith('auto');
  });

  it('highlights the current mode', () => {
    currentMode = 'plan';
    render(() => <ChatModeControl />);
    expect(screen.getByTestId('mode-plan').className).toContain('bg-primary');
    expect(screen.getByTestId('mode-normal').className).not.toContain('bg-primary');
  });
});

describe('nextChatMode', () => {
  it('cycles normal → plan → auto → normal', () => {
    expect(nextChatMode('normal')).toBe('plan');
    expect(nextChatMode('plan')).toBe('auto');
    expect(nextChatMode('auto')).toBe('normal');
  });
});
