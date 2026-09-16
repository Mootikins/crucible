import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@solidjs/testing-library';
import type { ModeDescriptor } from '@/lib/types';

const mockSwitchMode = vi.fn();
let currentMode = 'ask';
let modes: ModeDescriptor[] = [
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

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const trigger = () => screen.getByTestId('chat-mode');
const popout = () => screen.queryByTestId('chat-mode-popout');

// The control is a round icon button with a popover. The popover renders
// through a Portal into document.body, so every query goes through `screen`.
describe('ChatModeControl — the trigger', () => {
  it('is a circle that shows the mode as an icon and names it for a reader', () => {
    render(() => <ChatModeControl />);
    const button = trigger();
    expect(button.tagName).toBe('BUTTON');
    expect(button.classList).toContain('rounded-full');
    expect(button.classList).toContain('aspect-square');
    // The glyph carries the mode; there is no text label on the circle.
    expect(button.querySelector('svg')).toBeTruthy();
    expect((button.textContent ?? '').trim()).toBe('');
    expect(button.getAttribute('aria-label')).toBe('Mode: Ask');
    expect(button.getAttribute('title')).toContain('Ask');
  });

  it('wears the current mode, not the first one', () => {
    currentMode = 'plan';
    render(() => <ChatModeControl />);
    expect(trigger().getAttribute('aria-label')).toBe('Mode: Plan');
  });

  it('names a Lua-declared mode it has no icon for', () => {
    modes = [
      { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
      { id: 'review', name: 'Review', description: null, icon: null, color: null },
    ];
    currentMode = 'review';
    render(() => <ChatModeControl />);
    expect(trigger().getAttribute('aria-label')).toBe('Mode: Review');
  });
});

describe('ChatModeControl — opening and closing', () => {
  it('opens under the pointer and closes once the pointer has left', () => {
    vi.useFakeTimers();
    render(() => <ChatModeControl />);
    expect(popout()).toBeNull();

    fireEvent.mouseEnter(trigger());
    expect(popout()).toBeTruthy();

    // A short grace period lets the pointer travel from the circle into the
    // list; the list is not gone the instant the pointer crosses the gap.
    fireEvent.mouseLeave(trigger());
    expect(popout()).toBeTruthy();
    vi.advanceTimersByTime(400);
    expect(popout()).toBeNull();
  });

  it('stays open while the pointer rests on the list', () => {
    vi.useFakeTimers();
    render(() => <ChatModeControl />);
    fireEvent.mouseEnter(trigger());
    fireEvent.mouseLeave(trigger());
    fireEvent.mouseEnter(popout()!);
    vi.advanceTimersByTime(400);
    expect(popout()).toBeTruthy();
  });

  it('opens on click and closes on a second click', () => {
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    expect(popout()).toBeTruthy();
    expect(trigger().getAttribute('aria-expanded')).toBe('true');
    fireEvent.click(trigger());
    expect(popout()).toBeNull();
    expect(trigger().getAttribute('aria-expanded')).toBe('false');
  });

  it('closes on Escape', () => {
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    expect(popout()).toBeTruthy();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(popout()).toBeNull();
  });

  it('opens from the keyboard and picks with arrows and Enter', () => {
    render(() => <ChatModeControl />);
    fireEvent.keyDown(trigger(), { key: 'ArrowDown' });
    expect(popout()).toBeTruthy();
    // Opening highlights the current mode (Ask); one step down is Plan.
    fireEvent.keyDown(document, { key: 'ArrowDown' });
    fireEvent.keyDown(document, { key: 'Enter' });
    expect(mockSwitchMode).toHaveBeenCalledWith('plan');
    expect(popout()).toBeNull();
  });
});

describe('ChatModeControl — the rows', () => {
  it('draws a built-in mode as icon, name and description', () => {
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    for (const id of ['ask', 'plan', 'auto']) {
      const row = screen.getByTestId(`mode-${id}`);
      expect(row.querySelector('svg'), `${id} row has no icon`).toBeTruthy();
      expect(row.textContent?.length).toBeGreaterThan(0);
    }
    // Three built-ins, three different glyphs.
    const paths = ['ask', 'plan', 'auto'].map(
      (id) => screen.getByTestId(`mode-${id}`).querySelector('svg')!.innerHTML,
    );
    expect(new Set(paths).size).toBe(3);
    // A description under the name.
    expect(screen.getByTestId('mode-plan').textContent).toContain('Plan');
    expect(screen.getByTestId('mode-plan').textContent?.length).toBeGreaterThan('Plan'.length);
  });

  it('prefers the description the daemon sent', () => {
    modes = [
      { id: 'ask', name: 'Ask', description: 'Asks about everything', icon: null, color: null },
    ];
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    expect(screen.getByTestId('mode-ask').textContent).toContain('Asks about everything');
  });

  it('draws a Lua-declared mode as its name alone, with no icon slot', () => {
    modes = [
      { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
      { id: 'review', name: 'Review', description: null, icon: null, color: null },
    ];
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    const row = screen.getByTestId('mode-review');
    expect(row.querySelector('svg')).toBeNull();
    expect(row.textContent?.trim()).toBe('Review');
  });

  it('maps an icon name the daemon sent for a Lua-declared mode', () => {
    modes = [
      { id: 'review', name: 'Review', description: null, icon: 'eye', color: null },
    ];
    currentMode = 'review';
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    expect(screen.getByTestId('mode-review').querySelector('svg')).toBeTruthy();
    expect(trigger().querySelector('svg')).toBeTruthy();
  });

  it('checks the current mode in the list', () => {
    currentMode = 'plan';
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    expect(screen.getByTestId('mode-plan').getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('mode-ask').getAttribute('aria-selected')).toBe('false');
  });

  it('picking a mode persists it via switchMode and closes the list', () => {
    render(() => <ChatModeControl />);
    fireEvent.click(trigger());
    fireEvent.click(screen.getByTestId('mode-plan'));
    expect(mockSwitchMode).toHaveBeenCalledWith('plan');
    expect(popout()).toBeNull();

    fireEvent.click(trigger());
    fireEvent.click(screen.getByTestId('mode-auto'));
    expect(mockSwitchMode).toHaveBeenCalledWith('auto');
  });

  // A delegated (ACP) session reports the AGENT's modes, not Crucible's:
  // claude-agent-acp declares five camelCase ids, codex-acp three hyphenated
  // ones. The rows show the agent's human labels, and switching sends the id.
  it("renders an ACP agent's own modes by their declared names", () => {
    modes = [
      { id: 'default', name: 'Manual', description: null, icon: null, color: null },
      { id: 'acceptEdits', name: 'Accept edits', description: null, icon: null, color: null },
      { id: 'bypassPermissions', name: 'Bypass permissions', description: null, icon: null, color: null },
    ];
    currentMode = 'default';

    render(() => <ChatModeControl />);
    fireEvent.click(trigger());

    expect(screen.getByText('Accept edits')).toBeTruthy();
    expect(screen.getByText('Bypass permissions')).toBeTruthy();

    fireEvent.click(screen.getByTestId('mode-acceptEdits'));
    expect(mockSwitchMode).toHaveBeenCalledWith('acceptEdits');
  });
});

describe('nextChatMode', () => {
  // Cycling walks whatever the daemon reported, so an ACP set wraps within
  // itself and never lands on a Crucible mode the agent would reject.
  it("cycles within the ACP agent's set and wraps", () => {
    const claude = ['default', 'acceptEdits', 'plan', 'auto', 'bypassPermissions'];
    expect(nextChatMode('default', claude)).toBe('acceptEdits');
    expect(nextChatMode('bypassPermissions', claude)).toBe('default');
    expect(nextChatMode('ask', claude)).toBe('ask');
  });

  it('follows the order of the list it is given, not a fixed ring', () => {
    // Must not be a ROTATION of normal → plan → auto: a rotation has the same
    // successor for every element, so it passes against the old hardcoded ring
    // too. Swapping two entries is what discriminates.
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
    // Advancing would put the control in a mode `set_mode` rejects — the
    // control and the agent would then disagree with no way for the user to tell.
    expect(nextChatMode('review', ['ask', 'plan'])).toBe('review');
  });
});
