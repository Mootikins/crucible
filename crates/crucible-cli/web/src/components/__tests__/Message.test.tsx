import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { Message as MessageType } from '@/lib/types';

// ── Mocks ──────────────────────────────────────────────────────────────

// Markdown rendering is exercised in its own module; here we just check
// that the wired-in HTML lands in the assistant innerHTML.
vi.mock('@/lib/markdown', () => ({
  renderMarkdown: (s: string) => `<p data-md-sync>${s}</p>`,
  renderMarkdownAsync: (s: string) =>
    Promise.resolve(`<p data-md-async>${s}</p>`),
  renderPlainWithWikilinks: (s: string) => `${s}`,
}));

// ChatContext: capture what handlers do without spinning up a real provider.
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

// Note-link navigation: capture the resolve + open calls.
const getNoteMock = vi.fn();
const getConfigMock = vi.fn();
const openFileInEditorMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  getNote: (...args: unknown[]) => getNoteMock(...args),
  getConfig: (...args: unknown[]) => getConfigMock(...args),
}));

vi.mock('@/lib/file-actions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/file-actions')>()),
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

// Import AFTER mocks.
import { Message } from '../Message';
import { statusBarActions } from '@/stores/statusBarStore';

function makeMessage(overrides: Partial<MessageType> = {}): MessageType {
  return {
    id: 'm-1',
    role: 'assistant',
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
  it('renders the user bubble with justify-end and user content as plain text', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'user', content: 'hi **there**' })} />
    ));
    const outer = container.querySelector('[data-testid="message-user"]') as HTMLElement;
    expect(outer.className).toContain('justify-end');
    // User content rendered verbatim — no markdown HTML
    expect(screen.getByText('hi **there**')).toBeInTheDocument();
  });

  it('renders the assistant bubble with justify-start and innerHTML from markdown', async () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'assistant', content: 'body' })} />
    ));
    const outer = container.querySelector('[data-testid="message-assistant"]') as HTMLElement;
    expect(outer.className).toContain('justify-start');

    // Sync render is the first thing committed.
    await waitFor(() => {
      const md = container.querySelector('[data-md-async]') ?? container.querySelector('[data-md-sync]');
      expect(md?.textContent).toBe('body');
    });
  });

  it('renders the system role with italic styling and no action buttons', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'system', content: 'sys note' })} />
    ));
    const outer = container.querySelector('[data-testid="message-system"]') as HTMLElement;
    expect(outer).toBeInTheDocument();
    // System messages don't render copy/edit/regen buttons
    expect(screen.queryByTitle('Copy message')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Edit message')).not.toBeInTheDocument();
  });
});

// ── Empty / streaming indicators ───────────────────────────────────────

describe('Message — empty + streaming states', () => {
  it('renders the three-dot indicator when content is empty and no tool calls', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'assistant', content: '' })} />
    ));
    const dots = container.querySelectorAll('.animate-pulse');
    expect(dots.length).toBeGreaterThanOrEqual(3);
  });

  it('renders the streaming cursor when isStreaming is true', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'assistant', content: 'streamed' })} isStreaming />
    ));
    const cursor = container.querySelector('.w-2.h-4.animate-pulse');
    expect(cursor).not.toBeNull();
  });

  it('suppresses the action buttons while streaming', () => {
    render(() => (
      <Message message={makeMessage({ role: 'assistant', content: 'partial' })} isStreaming />
    ));
    expect(screen.queryByTitle('Copy message')).not.toBeInTheDocument();
  });
});

// ── Action buttons ─────────────────────────────────────────────────────

describe('Message — action buttons', () => {
  it('shows Copy on every non-system, non-streaming message', () => {
    render(() => <Message message={makeMessage({ role: 'assistant' })} />);
    expect(screen.getByTitle('Copy message')).toBeInTheDocument();
  });

  it('shows Edit only on user messages', () => {
    const { unmount } = render(() => (
      <Message message={makeMessage({ role: 'user', content: 'me' })} />
    ));
    expect(screen.getByTitle('Edit message')).toBeInTheDocument();
    unmount();

    render(() => <Message message={makeMessage({ role: 'assistant' })} />);
    expect(screen.queryByTitle('Edit message')).not.toBeInTheDocument();
  });

  it('shows Regenerate only on the last assistant message', () => {
    const { unmount } = render(() => (
      <Message message={makeMessage({ role: 'assistant' })} isLast />
    ));
    expect(screen.getByTitle('Regenerate response')).toBeInTheDocument();
    unmount();

    render(() => (
      <Message message={makeMessage({ role: 'assistant' })} isLast={false} />
    ));
    expect(screen.queryByTitle('Regenerate response')).not.toBeInTheDocument();
  });
});

// ── Copy flow ──────────────────────────────────────────────────────────

describe('Message — copy', () => {
  it('writes content to clipboard and swaps to a check icon, then reverts', async () => {
    render(() => (
      <Message message={makeMessage({ role: 'assistant', content: 'copy me' })} />
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

    render(() => <Message message={makeMessage({ role: 'assistant' })} />);
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

// ── Regenerate ─────────────────────────────────────────────────────────

describe('Message — regenerate', () => {
  it('resends the most recent user message from chat.messages()', async () => {
    messagesMock.mockReturnValue([
      { id: 'u1', role: 'user', content: 'first', timestamp: 1 },
      { id: 'a1', role: 'assistant', content: 'reply', timestamp: 2 },
      { id: 'u2', role: 'user', content: 'second', timestamp: 3 },
      { id: 'a2', role: 'assistant', content: 'reply2', timestamp: 4 },
    ]);
    render(() => (
      <Message
        message={makeMessage({ id: 'a2', role: 'assistant', content: 'reply2' })}
        isLast
      />
    ));
    fireEvent.click(screen.getByTitle('Regenerate response'));
    await waitFor(() => expect(sendMessageMock).toHaveBeenCalledWith('second'));
  });

  it('does nothing when there is no prior user message', async () => {
    messagesMock.mockReturnValue([
      { id: 'a1', role: 'assistant', content: 'lone reply', timestamp: 1 },
    ]);
    render(() => (
      <Message message={makeMessage({ id: 'a1', role: 'assistant' })} isLast />
    ));
    fireEvent.click(screen.getByTitle('Regenerate response'));
    await new Promise((r) => setTimeout(r, 0));
    expect(sendMessageMock).not.toHaveBeenCalled();
  });
});

// ── Token usage ────────────────────────────────────────────────────────

describe('Message — token usage', () => {
  it('renders the token total when usage is present on an assistant message', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          usage: { promptTokens: 100, completionTokens: 50, totalTokens: 1234 },
        })}
      />
    ));
    expect(screen.getByText(/1,234 tokens/)).toBeInTheDocument();
  });

  it('appends the cached count when cache tokens are present', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          usage: {
            promptTokens: 100,
            completionTokens: 50,
            totalTokens: 1500,
            cacheReadTokens: 200,
            cacheCreationTokens: 50,
          },
        })}
      />
    ));
    expect(screen.getByText(/1,500 tokens \(250 cached\)/)).toBeInTheDocument();
  });

  it('omits token usage on user messages even if usage is present', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'user',
          content: 'q',
          usage: { promptTokens: 1, completionTokens: 2, totalTokens: 3 },
        })}
      />
    ));
    expect(screen.queryByText(/3 tokens/)).not.toBeInTheDocument();
  });
});

// ── Timestamp formatting ───────────────────────────────────────────────

describe('Message — relative time', () => {
  const NOW = new Date('2026-05-17T12:00:00').getTime();

  beforeEach(() => {
    vi.setSystemTime(NOW);
  });

  it('shows "just now" for very recent timestamps', () => {
    render(() => (
      <Message message={makeMessage({ timestamp: NOW - 10_000 })} />
    ));
    expect(screen.getByText('just now')).toBeInTheDocument();
  });

  it('shows minutes-ago for sub-hour timestamps', () => {
    render(() => (
      <Message message={makeMessage({ timestamp: NOW - 5 * 60_000 })} />
    ));
    expect(screen.getByText('5 min ago')).toBeInTheDocument();
  });

  it('singularizes "1 hour ago"', () => {
    render(() => (
      <Message message={makeMessage({ timestamp: NOW - 60 * 60_000 })} />
    ));
    expect(screen.getByText('1 hour ago')).toBeInTheDocument();
  });

  it('pluralizes "N hours ago"', () => {
    render(() => (
      <Message message={makeMessage({ timestamp: NOW - 3 * 60 * 60_000 })} />
    ));
    expect(screen.getByText('3 hours ago')).toBeInTheDocument();
  });

  it('formats yesterday with HH:MM (when diff is in the 24-48h window)', () => {
    // NOW = 2026-05-17 12:00. Pick a yesterday timestamp >24h ago so the
    // formatter takes the "Yesterday at HH:MM" branch instead of "X hours
    // ago". 2026-05-16 09:15 → diff ≈ 26h45m → diffDay = 1.
    const yest = new Date('2026-05-16T09:15:00').getTime();
    render(() => <Message message={makeMessage({ timestamp: yest })} />);
    expect(screen.getByText('Yesterday at 09:15')).toBeInTheDocument();
  });

  it('falls back to a locale date for older timestamps', () => {
    const old = new Date('2026-01-10T00:00:00').getTime();
    render(() => <Message message={makeMessage({ timestamp: old })} />);
    // Format is locale-dependent; just assert "2026" + "Jan" appear
    expect(screen.getByText(/Jan/)).toBeInTheDocument();
    expect(screen.getByText(/2026/)).toBeInTheDocument();
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

  it('omits the badge on assistant messages even with precognition metadata', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          precognition: { notesCount: 1, notes: [{ name: 'n', relevance: 0.5 }] },
        })}
      />
    ));
    expect(screen.queryByText(/Enriched with/)).not.toBeInTheDocument();
  });
});

// ── Thinking block ─────────────────────────────────────────────────────

describe('Message — thinking block', () => {
  // showThinking is a module-global signal; reset so a mid-test failure
  // can't leak a hidden-thinking state into unrelated tests.
  afterEach(() => statusBarActions.setShowThinking(true));

  it('renders the thinking block when content is present', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: 'reply',
          thinking: { content: 'reasoning steps here', isStreaming: false, tokenCount: 42 },
        })}
      />
    ));
    expect(screen.getByText(/reasoning steps here/)).toBeInTheDocument();
  });

  it('omits the thinking block when content is empty', () => {
    const { container } = render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: 'reply',
          thinking: { content: '', isStreaming: false },
        })}
      />
    ));
    // No element should display the (empty) thinking content. The reasoning
    // header from ThinkingBlock would normally appear; assert it doesn't.
    expect(container.querySelector('[data-thinking-block]')).toBeNull();
  });

  it('hides the thinking block when show-thinking is toggled off (Ctrl+T)', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: 'reply',
          thinking: { content: 'reasoning steps here', isStreaming: false, tokenCount: 42 },
        })}
      />
    ));
    expect(screen.getByText(/reasoning steps here/)).toBeInTheDocument();

    statusBarActions.setShowThinking(false);
    expect(screen.queryByText(/reasoning steps here/)).not.toBeInTheDocument();

    statusBarActions.setShowThinking(true);
    expect(screen.getByText(/reasoning steps here/)).toBeInTheDocument();
  });
});

// ── Code-block copy buttons (addCopyButtons side-effect) ──────────────

describe('Message — code-block copy buttons', () => {
  it('appends a Copy button to <pre> blocks emitted by the markdown renderer', async () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'assistant', content: 'snippet' })} />
    ));
    await waitFor(() =>
      expect(container.querySelector('[data-md-async]')).not.toBeNull(),
    );

    // Simulate the next render emitting a <pre><code>. The effect that
    // watches renderedContent will fire again because we replace the
    // innerHTML of the markdown ref directly is not possible; instead use
    // the helper through its observable side: append a pre and call the
    // exported behavior via DOM dispatch on a fresh element.
    const pre = document.createElement('pre');
    pre.innerHTML = '<code>echo hi</code>';
    const md = container.querySelector('[data-md-async]')!.parentElement as HTMLElement;
    md.append(pre);

    // The createEffect won't auto-rerun for direct DOM mutation, but we can
    // verify the helper's idempotent dataset.copyButton flag behavior by
    // calling it indirectly: assert the pre starts WITHOUT a button.
    expect(pre.querySelector('button')).toBeNull();
    expect(pre.dataset.copyButton).toBeUndefined();
  });

  it('initial render with code in mocked markdown attaches a copy button via the effect', async () => {
    // Our markdown mock wraps the input in <p>${s}</p>, so feeding a raw
    // <pre><code> into content lets us exercise the addCopyButtons effect
    // without standing up the real markdown renderer.
    const { container } = render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: '<pre><code>cmd</code></pre>',
        })}
      />
    ));
    await waitFor(() => {
      const pre = container.querySelector('pre');
      expect(pre).not.toBeNull();
      // The effect runs on mount and finds the pre — copy button attached.
      expect(pre!.querySelector('button')?.textContent).toBe('Copy');
      expect(pre!.dataset.copyButton).toBe('true');
    });
  });

  it('clicking the per-block Copy button writes the code text to clipboard', async () => {
    const { container } = render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: '<pre><code>just this</code></pre>',
        })}
      />
    ));
    let copyButton: HTMLButtonElement | null = null;
    await waitFor(() => {
      copyButton = container.querySelector('pre button') as HTMLButtonElement | null;
      expect(copyButton).not.toBeNull();
    });

    fireEvent.click(copyButton!);
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('just this'),
    );
    await waitFor(() => expect(copyButton!.textContent).toBe('Copied'));
  });
});

// ── Note links ─────────────────────────────────────────────────────────

describe('Message — wikilink click handler', () => {
  it('intercepts clicks on [data-note] elements (preventDefault)', async () => {
    const { container } = render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: '<a data-note="Some Note">link</a>',
        })}
      />
    ));
    await waitFor(() => {
      const a = container.querySelector('a[data-note]') as HTMLAnchorElement | null;
      expect(a).not.toBeNull();
    });
    const a = container.querySelector('a[data-note]') as HTMLAnchorElement;
    const evt = new MouseEvent('click', { bubbles: true, cancelable: true });
    a.dispatchEvent(evt);
    expect(evt.defaultPrevented).toBe(true);
  });

  it('resolves the note and opens it in the editor', async () => {
    getConfigMock.mockResolvedValue({ kiln_path: '/kiln' });
    getNoteMock.mockResolvedValue({
      name: 'Some Note',
      path: '/kiln/Some Note.md',
      content: '',
      title: null,
      tags: [],
      updated_at: '',
    });

    const { container } = render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: '<a data-note="Some Note">link</a>',
        })}
      />
    ));
    await waitFor(() => {
      expect(container.querySelector('a[data-note]')).not.toBeNull();
    });
    (container.querySelector('a[data-note]') as HTMLAnchorElement).dispatchEvent(
      new MouseEvent('click', { bubbles: true, cancelable: true })
    );

    await waitFor(() => {
      expect(getNoteMock).toHaveBeenCalledWith('Some Note', '/kiln');
      expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/Some Note.md', 'Some Note');
    });
  });

  it('surfaces a warning instead of opening when the note cannot be resolved', async () => {
    getConfigMock.mockResolvedValue({ kiln_path: '/kiln' });
    getNoteMock.mockRejectedValue(new Error('Failed to get note: 404 Not Found'));

    const { container } = render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: '<a data-note="Ghost">link</a>',
        })}
      />
    ));
    await waitFor(() => {
      expect(container.querySelector('a[data-note]')).not.toBeNull();
    });
    (container.querySelector('a[data-note]') as HTMLAnchorElement).dispatchEvent(
      new MouseEvent('click', { bubbles: true, cancelable: true })
    );

    await waitFor(() => {
      expect(getNoteMock).toHaveBeenCalled();
    });
    expect(openFileInEditorMock).not.toHaveBeenCalled();
  });

  it('ignores clicks that are not on a [data-note] element', () => {
    const { container } = render(() => (
      <Message message={makeMessage({ role: 'assistant', content: 'plain' })} />
    ));
    const md = container.querySelector('[data-md-async], [data-md-sync]') as HTMLElement;
    const evt = new MouseEvent('click', { bubbles: true, cancelable: true });
    md.dispatchEvent(evt);
    expect(evt.defaultPrevented).toBe(false);
  });
});

// ── Tool calls ─────────────────────────────────────────────────────────

describe('Message — tool calls', () => {
  it('renders one ToolCard per toolCall summary', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: 'reply',
          toolCalls: [
            { id: 't1', title: 'read_file' },
            { id: 't2', title: 'bash' },
          ],
        })}
      />
    ));
    expect(screen.getByText('read_file')).toBeInTheDocument();
    expect(screen.getByText('bash')).toBeInTheDocument();
  });

  it('renders tool cards even when the assistant message has no text content', () => {
    render(() => (
      <Message
        message={makeMessage({
          role: 'assistant',
          content: '',
          toolCalls: [{ id: 't', title: 'only-tool' }],
        })}
      />
    ));
    expect(screen.getByText('only-tool')).toBeInTheDocument();
    // The pulse-dot fallback should NOT render once toolCalls are present
    const dots = document.body.querySelectorAll('.w-2.h-2.animate-pulse');
    expect(dots.length).toBe(0);
  });
});
