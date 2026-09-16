import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
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
// Message uses chat.sendMessage for the edit "Send as new" flow.
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
    chatMode: () => 'ask',
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
    // the full-width quoted block — the distinguishing class is user-quote.
    expect(outer.querySelector('.user-quote')).not.toBeNull();
    // User content rendered verbatim — no markdown HTML
    expect(screen.getByText('hi **there**')).toBeInTheDocument();
  });

  it('renders the system role with italic styling and no action buttons', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'system', content: 'sys note' })} />
    ));
    const outer = container.querySelector('[data-testid="message-system"]') as HTMLElement;
    expect(outer).toBeInTheDocument();
    // The column aligns to the trailing edge for a prompt; a notice spans
    // the whole measure, so the edge cannot show on it.
    const notice = outer.firstElementChild as HTMLElement;
    expect(notice.className).toContain('w-full');
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
    fireEvent.click(screen.getByText('Send as new'));

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
    fireEvent.click(screen.getByText('Send as new'));

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

  it('shows the sent time at the end of the prompt', async () => {
    const ts = NOW - 5 * 60_000;
    render(() => <Message message={{ id: 'u1', role: 'user', content: 'hi', timestamp: ts }} />);
    const { formatMessageTime } = await import('@/lib/format-time');
    const time = screen.getByTestId('message-time');
    expect(time.textContent).toBe(formatMessageTime(ts, NOW));
  });
});

// ── The meta row under the prompt ─────────────────────────────────────

describe('Message — the meta row under the prompt', () => {
  it('stacks under the bubble on the trailing edge, and draws no fixed side column', () => {
    const { container } = render(() => <Message message={makeMessage({ role: 'user' })} />);
    const row = container.querySelector('[data-testid="message-user"]') as HTMLElement;
    const bubble = container.querySelector('.user-quote') as HTMLElement;
    const meta = screen.getByTestId('turn-meta');
    expect(row.className).toContain('flex-col');
    expect(row.className).toContain('items-end');
    expect(row.className).not.toContain('items-start');
    expect(bubble.compareDocumentPosition(meta) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The row's contents sit at the trailing edge too.
    expect(meta.className).toContain('justify-end');
    // The right-hand column is gone, in the markup and in the tokens.
    expect(screen.queryByTestId('turn-gutter')).toBeNull();
    expect(meta.className).not.toContain('cru-turn-gutter');
    expect(meta.className).not.toContain('absolute');
    const css = readFileSync(resolve(__dirname, '../../index.css'), 'utf-8');
    expect(css).not.toContain('--cru-turn-gutter');
  });

  it('renders every action through IconButton, none hand-drawn', () => {
    render(() => <Message message={makeMessage({ role: 'user' })} />);
    const meta = screen.getByTestId('turn-meta');
    for (const title of ['Copy message', 'Edit message']) {
      const button = screen.getByTitle(title);
      expect(meta).toContainElement(button);
      // IconButton's dense box — a 24px square with a shared hover wash, not
      // a bare `rounded p-1` re-drawn per call site.
      expect(button.className).toContain('w-6');
      expect(button.className).toContain('h-6');
      expect(button.className).toContain('hover:bg-hover-wash');
      expect(button.className).not.toMatch(/(^|\s)p-1(\s|$)/);
    }
  });

  it('puts the sent time first and the actions at the outer edge', () => {
    render(() => <Message message={makeMessage({ role: 'user', timestamp: Date.now() })} />);
    const meta = screen.getByTestId('turn-meta');
    const time = screen.getByTestId('message-time');
    expect(meta).toContainElement(time);
    const copy = screen.getByTitle('Copy message');
    expect(time.compareDocumentPosition(copy) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('is hidden at rest and fades in on hover or keyboard focus', () => {
    render(() => <Message message={makeMessage({ role: 'user', timestamp: Date.now() })} />);
    const meta = screen.getByTestId('turn-meta');
    expect(meta.className).toContain('opacity-0');
    expect(meta.className).toContain('transition-opacity');
    expect(meta.className).toContain('group-hover:opacity-100');
    expect(meta.className).toContain('group-focus-within:opacity-100');
    // A phone has no hover, so the row cannot be gated behind one there.
    expect(meta.className).toContain('[@media(hover:none)]:opacity-100');
  });

  it('takes no click while it is hidden, and takes one again when it shows', () => {
    // An invisible button that still hit-tests is a trap: a tap on a device
    // that reports hover lands on Copy or Edit with nothing on screen.
    render(() => <Message message={makeMessage({ role: 'user', timestamp: Date.now() })} />);
    const meta = screen.getByTestId('turn-meta');
    expect(meta.className).toContain('pointer-events-none');
    expect(meta.className).toContain('group-hover:pointer-events-auto');
    expect(meta.className).toContain('group-focus-within:pointer-events-auto');
    expect(meta.className).toContain('[@media(hover:none)]:pointer-events-auto');
  });

  it('changes NOTHING but opacity between rest and hover', () => {
    // The row is always in the flow. Any other state-gated utility — a
    // margin, a display, a position — would move an edge under the pointer.
    // Pointer events move nothing, so the hit-test guard is allowed too.
    render(() => <Message message={makeMessage({ role: 'user', timestamp: Date.now() })} />);
    const meta = screen.getByTestId('turn-meta');
    const stateGated = meta.className
      .split(/\s+/)
      .filter((c) => /^(group-hover:|group-focus-within:|\[@media\(hover:none\)\]:)/.test(c))
      // The media variant carries a colon of its own, so the UTILITY is
      // what follows the LAST one.
      .map((c) => c.slice(c.lastIndexOf(':') + 1));
    expect(stateGated.length).toBeGreaterThan(0);
    expect(
      stateGated.filter((c) => !c.startsWith('opacity-') && !c.startsWith('pointer-events-')),
    ).toEqual([]);
  });

  it('leaves the bubble box identical with and without a sent time', () => {
    const withTime = render(() => (
      <Message message={makeMessage({ role: 'user', content: 'a prompt', timestamp: Date.now() })} />
    ));
    const stamped = withTime.container.querySelector('.user-quote') as HTMLElement;
    // The stamp is OUTSIDE the bubble now, so the bubble cannot answer for it.
    expect(stamped.querySelector('[data-testid="message-time"]')).toBeNull();
    const stampedClass = stamped.className;
    const stampedHtml = stamped.innerHTML;
    withTime.unmount();

    const without = render(() => (
      <Message message={makeMessage({ role: 'user', content: 'a prompt', timestamp: 0 })} />
    ));
    const bare = without.container.querySelector('.user-quote') as HTMLElement;
    expect(bare.className).toBe(stampedClass);
    expect(bare.innerHTML).toBe(stampedHtml);
  });

  it('announces the author as a heading for a screen reader only', () => {
    const { container } = render(() => <Message message={makeMessage({ role: 'user' })} />);
    const heading = container.querySelector('h3') as HTMLElement;
    expect(heading).not.toBeNull();
    expect(heading.textContent).toBe('You');
    expect(heading.className).toContain('sr-only');
  });

  it('draws no meta row on a system notice', () => {
    render(() => <Message message={makeMessage({ role: 'system', content: 'sys' })} />);
    expect(screen.queryByTestId('turn-meta')).toBeNull();
  });
});

// ── The bubble's width ─────────────────────────────────────

describe('Message — the prompt sizes to its text', () => {
  it('declares fit-content with a clamp, not a full-width block', () => {
    // Read the rule rather than the class list: `.user-quote` is a component
    // rule, and the draft composer's pending preview wears the same class, so
    // the two surfaces cannot drift apart.
    const css = readFileSync(resolve(__dirname, '../../index.css'), 'utf-8');
    const rule = /\.user-quote\s*\{([\s\S]*?)\}/.exec(css);
    expect(rule, '.user-quote is missing from index.css').not.toBeNull();
    expect(rule![1]).toMatch(/width:\s*fit-content/);
    expect(rule![1]).toMatch(/max-width:\s*100%/);
    // A bare `width: 100%` — the rule this pass replaced. `max-width` is
    // the clamp and must survive, so the boundary rejects the hyphen.
    expect(rule![1]).not.toMatch(/(?<!-)width:\s*100%/);
  });

  it('draws a whole-pixel box, so the rows under it land on whole pixels too', () => {
    // A 35.6px bubble put every row after it on a .6 pixel.
    const css = readFileSync(resolve(__dirname, '../../index.css'), 'utf-8');
    const rule = /\.user-quote\s*\{([\s\S]*?)\}/.exec(css)!;
    expect(rule[1]).toMatch(/padding:\s*0\.375rem 0\.6875rem/);
    expect(rule[1]).toMatch(/line-height:\s*1\.25rem/);
    // The prose beside it: 1.6 on 13px is 20.8px; 21px keeps the answer's
    // rows on whole pixels as well.
    expect(css).toMatch(/--cru-leading-reading:\s*1\.3125rem/);
  });
});

// ── The row's own rhythm ──────────────────────────────────

describe('Message — the row spends no vertical space of its own', () => {
  it('reserves no footer band and adds no margin', () => {
    const { container } = render(() => <Message message={makeMessage({ role: 'user' })} />);
    const row = container.querySelector('[data-testid="message-user"]') as HTMLElement;
    expect(row.className).not.toMatch(/\bpb-5\b/);
    expect(row.className).not.toMatch(/\bmb-\d/);
    expect(row.className).toContain('gap-1');
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
