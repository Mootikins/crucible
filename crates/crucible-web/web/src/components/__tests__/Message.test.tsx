import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { Message as MessageType } from '@/lib/types';

// ── Mocks ──────────────────────────────────────────────────────────────
//
// Message now renders USER + SYSTEM rows only — assistant output moved to
// AssistantTurn. User bubbles show text verbatim with wikilinks turned into
// anchors, so the only markdown helper Message touches is
// renderPlainWithWikilinks. Keep the mock to exactly what the component
// imports.
vi.mock('@/lib/markdown', () => ({
  renderPlainWithWikilinks: (s: string) => s,
}));

// ChatContext: capture what handlers do without spinning up a real provider.
// Message uses chat.sendMessage for the edit "Save & Send" flow.
const sendMessageMock = vi.fn().mockResolvedValue(undefined);
const messagesMock = vi.fn<() => MessageType[]>(() => []);

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    messages: messagesMock,
    isLoading: () => false,
    isStreaming: () => false,
    pendingInteraction: () => null,
    error: () => null,
    activeTools: () => [],
    subagentEvents: () => [],
    contextUsage: () => null,
    chatMode: () => 'normal',
    isLoadingHistory: () => false,
    setChatMode: () => {},
    sendMessage: (...args: unknown[]) => sendMessageMock(...args),
    respondToInteraction: async () => {},
    clearMessages: () => {},
    cancelStream: async () => {},
    addSystemMessage: () => {},
  }),
}));

// Import AFTER mocks.
import { Message } from '../Message';
import { formatRelativeTime } from '@/lib/format-time';

function makeMessage(overrides: Partial<MessageType> = {}): MessageType {
  return {
    id: 'm-1',
    role: 'user',
    content: 'hello',
    timestamp: Date.now(),
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  messagesMock.mockReturnValue([]);
  // Clipboard polyfill — overwrite per test
  Object.assign(navigator, {
    clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

// ── Role rendering ─────────────────────────────────────────────────────

describe('Message — role rendering', () => {
  it('renders the user prompt as a full-width quoted block and its content as plain text', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'user', content: 'hi **there**' })} />
    ));
    const outer = container.querySelector('[data-testid="message-user"]') as HTMLElement;
    // The prompt is a full-width quoted block (ember gutter), no longer a
    // right-aligned bubble — the distinguishing class is message-bubble-user.
    expect(outer.querySelector('.message-bubble-user')).not.toBeNull();
    // User content rendered verbatim — no markdown HTML
    expect(screen.getByText('hi **there**')).toBeInTheDocument();
  });

  it('renders the system role with italic styling and no action buttons', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'system', content: 'sys note' })} />
    ));
    const outer = container.querySelector('[data-testid="message-system"]') as HTMLElement;
    expect(outer).toBeInTheDocument();
    expect(outer.className).toContain('justify-start');
    expect(screen.getByText('sys note')).toBeInTheDocument();
    // System messages don't render copy/edit buttons
    expect(screen.queryByTitle('Copy message')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Edit message')).not.toBeInTheDocument();
  });
});

// ── Action buttons ─────────────────────────────────────────────────────

describe('Message — action buttons', () => {
  it('shows Copy and Edit on user messages', () => {
    render(() => <Message message={makeMessage({ role: 'user', content: 'me' })} />);
    expect(screen.getByTitle('Copy message')).toBeInTheDocument();
    expect(screen.getByTitle('Edit message')).toBeInTheDocument();
  });

  it('shows no action buttons on system messages', () => {
    render(() => <Message message={makeMessage({ role: 'system', content: 'sys' })} />);
    expect(screen.queryByTitle('Copy message')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Edit message')).not.toBeInTheDocument();
  });
});

// ── Copy flow ──────────────────────────────────────────────────────────

describe('Message — copy', () => {
  it('writes content to clipboard and swaps to a check icon, then reverts', async () => {
    render(() => (
      <Message message={makeMessage({ role: 'user', content: 'copy me' })} />
    ));

    const button = screen.getByTitle('Copy message');
    fireEvent.click(button);

    await waitFor(() => {
      expect((navigator.clipboard.writeText as ReturnType<typeof vi.fn>)).toHaveBeenCalledWith('copy me');
    });
    await waitFor(() => expect(screen.getByTitle('Copied!')).toBeInTheDocument());

    // Advance past the 2s revert delay
    vi.advanceTimersByTime(2100);
    await waitFor(() => expect(screen.getByTitle('Copy message')).toBeInTheDocument());
  });

  it('silently survives a clipboard failure', async () => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockRejectedValue(new Error('denied')) },
    });

    render(() => <Message message={makeMessage({ role: 'user' })} />);
    fireEvent.click(screen.getByTitle('Copy message'));

    // No throw, no Copied state
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalled(),
    );
    expect(screen.queryByTitle('Copied!')).not.toBeInTheDocument();
  });
});

// ── Edit flow ──────────────────────────────────────────────────────────

describe('Message — edit', () => {
  it('opens a textarea with current content and cancels on Escape', () => {
    render(() => (
      <Message message={makeMessage({ role: 'user', content: 'original' })} />
    ));

    fireEvent.click(screen.getByTitle('Edit message'));
    const textarea = screen.getByDisplayValue('original') as HTMLTextAreaElement;
    expect(textarea).toBeInTheDocument();

    fireEvent.keyDown(textarea, { key: 'Escape' });
    expect(screen.queryByDisplayValue('original')).not.toBeInTheDocument();
  });

  it('submits trimmed edit content via sendMessage', async () => {
    render(() => (
      <Message message={makeMessage({ role: 'user', content: 'first' })} />
    ));
    fireEvent.click(screen.getByTitle('Edit message'));
    const textarea = screen.getByDisplayValue('first') as HTMLTextAreaElement;
    fireEvent.input(textarea, { target: { value: '  revised  ' } });
    fireEvent.click(screen.getByText('Save & Send'));

    await waitFor(() => expect(sendMessageMock).toHaveBeenCalledWith('revised'));
    // Editor closes after submit
    expect(screen.queryByDisplayValue('  revised  ')).not.toBeInTheDocument();
  });

  it('does not submit when the edited content is empty/whitespace', async () => {
    render(() => (
      <Message message={makeMessage({ role: 'user', content: 'something' })} />
    ));
    fireEvent.click(screen.getByTitle('Edit message'));
    const textarea = screen.getByDisplayValue('something') as HTMLTextAreaElement;
    fireEvent.input(textarea, { target: { value: '   ' } });
    fireEvent.click(screen.getByText('Save & Send'));

    await new Promise((r) => setTimeout(r, 0));
    expect(sendMessageMock).not.toHaveBeenCalled();
  });
});

// ── Timestamp formatting ───────────────────────────────────────────────
//
// formatRelativeTime moved to @/lib/format-time. The matrix is tested
// directly against the helper (locale-robust); one case is also verified
// end-to-end through the user bubble to prove the wiring.

describe('formatRelativeTime', () => {
  const NOW = new Date('2026-05-17T12:00:00').getTime();

  beforeEach(() => {
    vi.setSystemTime(NOW);
  });

  it('shows "just now" for very recent timestamps', () => {
    expect(formatRelativeTime(NOW - 10_000)).toBe('just now');
  });

  it('shows minutes-ago for sub-hour timestamps', () => {
    expect(formatRelativeTime(NOW - 5 * 60_000)).toBe('5 min ago');
  });

  it('singularizes "1 hour ago"', () => {
    expect(formatRelativeTime(NOW - 60 * 60_000)).toBe('1 hour ago');
  });

  it('pluralizes "N hours ago"', () => {
    expect(formatRelativeTime(NOW - 3 * 60 * 60_000)).toBe('3 hours ago');
  });

  it('formats yesterday with HH:MM (when diff is in the 24-48h window)', () => {
    // NOW = 2026-05-17 12:00. Pick a yesterday timestamp >24h ago so the
    // formatter takes the "Yesterday at HH:MM" branch instead of "X hours
    // ago". 2026-05-16 09:15 → diff ≈ 26h45m → diffDay = 1.
    const yest = new Date('2026-05-16T09:15:00').getTime();
    expect(formatRelativeTime(yest)).toBe('Yesterday at 09:15');
  });

  it('falls back to a locale date for older timestamps', () => {
    const old = new Date('2026-01-10T00:00:00').getTime();
    // The month name and separators vary by ICU locale ("Jan" is English
    // only), so build the expected string exactly as the helper does rather
    // than asserting English-specific text.
    const expected = new Date(old).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
    expect(formatRelativeTime(old)).toBe(expected);
  });

  it('renders the relative time inside the user bubble', () => {
    render(() => (
      <Message message={makeMessage({ role: 'user', timestamp: NOW - 5 * 60_000 })} />
    ));
    expect(screen.getByText('5 min ago')).toBeInTheDocument();
  });
});

// ── Precognition badge ─────────────────────────────────────────────────

describe('Message — precognition badge', () => {
  it('renders the badge when a user message carries precognition metadata', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'user',
          content: 'q',
          precognition: { notesCount: 2, notes: [
            { name: 'note-a', relevance: 0.9 },
            { name: 'note-b', relevance: 0.8 },
          ] },
        })}
      />
    ));
    expect(screen.getByText(/Enriched with 2 notes/)).toBeInTheDocument();
  });
});
