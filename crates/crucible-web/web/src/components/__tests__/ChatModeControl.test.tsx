import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

const mockSwitchMode = vi.fn();
let currentMode = 'ask';
let modes = [
  { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
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
  currentMode = 'ask';
  modes = [
    { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
    { id: 'plan', name: 'Plan', description: null, icon: null, color: null },
    { id: 'auto', name: 'Auto', description: null, icon: null, color: null },
  ];
});

// The control is now the launchpad's ChipSelect dropdown (popout renders
// through a Portal into document.body — query via screen).
describe('ChatModeControl', () => {
  // The composer row carries several chips at once, so each states the axis
  // it sets as well as the value it holds.
  it('carries the value only; the icon and the tooltip name the axis', () => {
    render(() => <ChatModeControl />);
    const text = screen.getByTestId('chat-mode').textContent ?? '';
    expect(text).toContain('Ask');
    expect(text).not.toContain('Mode ·');
  });

  // A delegated (ACP) session reports the AGENT's modes, not Crucible's:
  // claude-agent-acp declares five camelCase ids, codex-acp three hyphenated
  // ones. The control renders `m.name`, so the dropdown must show the
  // agent's human labels rather than raw ids, and switching must send the id.
  it("renders an ACP agent's own modes by their declared names", () => {
    modes = [
      { id: 'default', name: 'Manual', description: null, icon: null, color: null },
      { id: 'acceptEdits', name: 'Accept edits', description: null, icon: null, color: null },
      { id: 'bypassPermissions', name: 'Bypass permissions', description: null, icon: null, color: null },
    ];
    currentMode = 'default';

    render(() => <ChatModeControl />);
    fireEvent.click(screen.getByTestId('chat-mode'));

    expect(screen.getByText('Accept edits')).toBeTruthy();
    expect(screen.getByText('Bypass permissions')).toBeTruthy();

    fireEvent.click(screen.getByTestId('mode-acceptEdits'));
    expect(mockSwitchMode).toHaveBeenCalledWith('acceptEdits');
  });

  // Cycling walks whatever the daemon reported, so an ACP set wraps within
  // itself and never lands on a Crucible mode the agent would reject.
  it("cycles within the ACP agent's set and wraps", () => {
    const claude = ['default', 'acceptEdits', 'plan', 'auto', 'bypassPermissions'];
    expect(nextChatMode('default', claude)).toBe('acceptEdits');
    expect(nextChatMode('bypassPermissions', claude)).toBe('default');
    expect(nextChatMode('ask', claude)).toBe('ask');
  });

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
    expect(screen.getByTestId('mode-ask').getAttribute('aria-selected')).toBe('false');
  });
});

describe('nextChatMode', () => {
  it('follows the order of the list it is given, not a fixed ring', () => {
    // Must not be a ROTATION of normal → plan → auto: a rotation has the same
    // successor for every element, so it passes against the old hardcoded ring
    // too. (My first attempt at this test used ['auto','ask','plan'] and was
    // exactly that.) Swapping two entries is what discriminates.
    const swapped = ['ask', 'auto', 'plan'];
    expect(nextChatMode('ask', swapped)).toBe('auto');
    expect(nextChatMode('auto', swapped)).toBe('plan');
    expect(nextChatMode('plan', swapped)).toBe('ask');
  });

  it('walks the daemon list, so a Lua-declared mode is reachable', () => {
    const declared = ['ask', 'review'];
    expect(nextChatMode('ask', declared)).toBe('review');
    expect(nextChatMode('review', declared)).toBe('ask');
  });

  it('leaves a mode the daemon no longer offers alone', () => {
    // Advancing would put the chip in a mode `set_mode` rejects — the chip
    // and the agent would then disagree with no way for the user to tell.
    expect(nextChatMode('review', ['ask', 'plan'])).toBe('review');
  });
});

describe('ChatModeControl with a Lua-declared mode', () => {
  it('offers and displays a mode the frontend has no constant for', () => {
    modes = [
      { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
      { id: 'review', name: 'Review', description: null, icon: null, color: null },
    ];
    currentMode = 'review';
    render(() => <ChatModeControl />);

    expect(screen.getByTestId('chat-mode').textContent).toContain('Review');
    fireEvent.click(screen.getByTestId('chat-mode'));
    expect(screen.getByTestId('mode-review').getAttribute('aria-selected')).toBe('true');
  });
});
