import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

const mockSwitchMode = vi.fn();
let currentMode = 'normal';
let modes = [
  { id: 'normal', name: 'Normal', description: null, icon: null, color: null },
  { id: 'plan', name: 'Plan', description: null, icon: null, color: null },
  { id: 'auto', name: 'Auto', description: null, icon: null, color: null },
];

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    chatMode: () => currentMode,
    availableModes: () => modes,
    switchMode: mockSwitchMode,
  }),
}));

import { ChatModeControl, nextChatMode } from '../ChatModeControl';

beforeEach(() => {
  vi.clearAllMocks();
  currentMode = 'normal';
  modes = [
    { id: 'normal', name: 'Normal', description: null, icon: null, color: null },
    { id: 'plan', name: 'Plan', description: null, icon: null, color: null },
    { id: 'auto', name: 'Auto', description: null, icon: null, color: null },
  ];
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
  it('follows the order of the list it is given, not a fixed ring', () => {
    // Must not be a ROTATION of normal → plan → auto: a rotation has the same
    // successor for every element, so it passes against the old hardcoded ring
    // too. (My first attempt at this test used ['auto','normal','plan'] and was
    // exactly that.) Swapping two entries is what discriminates.
    const swapped = ['normal', 'auto', 'plan'];
    expect(nextChatMode('normal', swapped)).toBe('auto');
    expect(nextChatMode('auto', swapped)).toBe('plan');
    expect(nextChatMode('plan', swapped)).toBe('normal');
  });

  it('walks the daemon list, so a Lua-declared mode is reachable', () => {
    const declared = ['normal', 'review'];
    expect(nextChatMode('normal', declared)).toBe('review');
    expect(nextChatMode('review', declared)).toBe('normal');
  });

  it('leaves a mode the daemon no longer offers alone', () => {
    // Advancing would put the chip in a mode `set_mode` rejects — the chip
    // and the agent would then disagree with no way for the user to tell.
    expect(nextChatMode('review', ['normal', 'plan'])).toBe('review');
  });
});

describe('ChatModeControl with a Lua-declared mode', () => {
  it('offers and displays a mode the frontend has no constant for', () => {
    modes = [
      { id: 'normal', name: 'Normal', description: null, icon: null, color: null },
      { id: 'review', name: 'Review', description: null, icon: null, color: null },
    ];
    currentMode = 'review';
    render(() => <ChatModeControl />);

    expect(screen.getByTestId('chat-mode').textContent).toContain('Review');
    fireEvent.click(screen.getByTestId('chat-mode'));
    expect(screen.getByTestId('mode-review').getAttribute('aria-selected')).toBe('true');
  });
});
