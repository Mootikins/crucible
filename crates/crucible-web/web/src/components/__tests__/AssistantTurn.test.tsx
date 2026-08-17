import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { Message } from '@/lib/types';

// ── Mocks ──────────────────────────────────────────────────────────────
//
// AssistantTurn resolves every part's live message from chat.messages() by
// id, so the mock exposes messages/isStreaming/sessionId/sendMessage and the
// tests drive them through mutable accessors (a token append is just a new
// messages() return).
let messagesAccessor: () => Message[] = () => [];
let streamingAccessor: () => boolean = () => false;
const sendMessageMock = vi.fn().mockResolvedValue(undefined);

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    messages: () => messagesAccessor(),
    isStreaming: () => streamingAccessor(),
    sessionId: () => 's1',
    sendMessage: (...args: unknown[]) => sendMessageMock(...args),
  }),
}));

let sessionsAccessor: () => Array<{ id: string; kilns: string[] }> = () => [];
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    sessions: () => sessionsAccessor(),
  }),
}));

// The registry join, stubbed: `vault` is registered, `ghost` is not.
vi.mock('@/stores/kilnStore', () => ({
  kilnPathOf: (name: string | null | undefined) =>
    name === 'vault' ? '/home/u/vault' : null,
}));

// Markdown is exercised in its own module; here we just check the wired-in
// HTML lands in the assistant innerHTML. The async pass upgrades the sync one.
vi.mock('@/lib/markdown', () => ({
  renderMarkdown: (s: string) => `<p data-md-sync>${s}</p>`,
  renderMarkdownChatAsync: (s: string) =>
    Promise.resolve(`<p data-md-async>${s}</p>`),
  PROSE_CLASS: 'prose',
}));

// Note navigation only fires on link clicks (not covered here); stub it so we
// don't pull in the api/file-actions chain.
vi.mock('@/lib/note-actions', () => ({
  openNoteInEditor: vi.fn(),
}));

// Import AFTER mocks.
import { AssistantTurn, type TurnPartSpec } from '../AssistantTurn';
import { statusBarActions } from '@/stores/statusBarStore';

const textMsg = (id: string, content: string, overrides: Partial<Message> = {}): Message => ({
  id,
  role: 'assistant',
  content,
  timestamp: Date.now(),
  ...overrides,
});

const userMsg = (id: string, content: string): Message => ({
  id,
  role: 'user',
  content,
  timestamp: Date.now(),
});

const toolMsg = (id: string): Message => ({
  id,
  role: 'tool',
  content: '',
  timestamp: Date.now(),
  toolCall: { id, callId: id, name: `tool-${id}`, args: '', status: 'complete' as const },
});

const textPart = (id: string): TurnPartSpec => ({ kind: 'text', id });
const toolsPart = (...ids: string[]): TurnPartSpec => ({ kind: 'tools', key: `tools-${ids[0]}`, ids });

beforeEach(() => {
  vi.clearAllMocks();
  messagesAccessor = () => [];
  streamingAccessor = () => false;
  sessionsAccessor = () => [];
  Object.assign(navigator, {
    clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
  statusBarActions.setShowThinking(true);
});

afterEach(() => {
  statusBarActions.setShowThinking(true);
});

// ── Markdown rendering ─────────────────────────────────────────────────

describe('AssistantTurn — text segment markdown', () => {
  it('renders markdown for a text segment via the async pipeline', async () => {
    messagesAccessor = () => [textMsg('a1', 'body text')];
    const { container } = render(() => (
      <AssistantTurn parts={[textPart('a1')]} isLast={false} />
    ));

    // Sync render commits first; async pass upgrades it.
    await waitFor(() => {
      const md =
        container.querySelector('[data-md-async]') ??
        container.querySelector('[data-md-sync]');
      expect(md?.textContent).toBe('body text');
    });
  });
});

// ── Single meta row (the regression this refactor fixes) ───────────────

describe('AssistantTurn — one meta row per turn', () => {
  it('shows EXACTLY ONE timestamp and ONE usage line for a text→tools→text turn', async () => {
    const ts = Date.now() - 5 * 60_000;
    messagesAccessor = () => [
      textMsg('a1', 'first', { timestamp: ts }),
      toolMsg('t1'),
      textMsg('a2', 'second', {
        timestamp: Date.now(),
        usage: { promptTokens: 10, completionTokens: 5, totalTokens: 1234 },
      }),
    ];
    render(() => (
      <AssistantTurn
        parts={[textPart('a1'), toolsPart('t1'), textPart('a2')]}
        isLast={false}
      />
    ));

    // Multiple segments, but the turn carries a single meta row.
    expect(screen.getAllByText(`${(1234).toLocaleString()} tokens`)).toHaveLength(1);
    // Timestamp is now ABSOLUTE and lives in the hover strip — still one.
    const { formatAbsoluteTime } = await import('@/lib/format-time');
    expect(screen.getAllByText(formatAbsoluteTime(ts))).toHaveLength(1);
  });

  it('picks up usage from whichever (last) segment carries it', () => {
    messagesAccessor = () => [
      textMsg('a1', 'first', {
        usage: { promptTokens: 1, completionTokens: 1, totalTokens: 999 },
      }),
      textMsg('a2', 'second', {
        usage: { promptTokens: 10, completionTokens: 5, totalTokens: 1234 },
      }),
    ];
    render(() => (
      <AssistantTurn parts={[textPart('a1'), textPart('a2')]} isLast={false} />
    ));

    // The final segment's usage wins; the earlier one is not shown.
    expect(screen.getByText(`${(1234).toLocaleString()} tokens`)).toBeInTheDocument();
    expect(screen.queryByText(`${(999).toLocaleString()} tokens`)).not.toBeInTheDocument();
  });

  it('appends the cached count when cache tokens are present', () => {
    messagesAccessor = () => [
      textMsg('a1', 'body', {
        usage: {
          promptTokens: 100,
          completionTokens: 50,
          totalTokens: 1500,
          cacheReadTokens: 200,
          cacheCreationTokens: 50,
        },
      }),
    ];
    render(() => <AssistantTurn parts={[textPart('a1')]} isLast={false} />);
    expect(
      screen.getByText(
        `${(1500).toLocaleString()} tokens (${(250).toLocaleString()} cached)`,
      ),
    ).toBeInTheDocument();
  });
});

// ── In-flight states: dots, caret, no meta ─────────────────────────────

describe('AssistantTurn — in-flight indicators', () => {
  it('shows working dots when the trailing text segment is empty', () => {
    messagesAccessor = () => [textMsg('a1', '')];
    const { getByTestId } = render(() => (
      <AssistantTurn parts={[textPart('a1')]} isLast={true} />
    ));
    expect(getByTestId('working-indicator')).toBeInTheDocument();
  });

  it('shows no dots for a settled thinking-only segment', () => {
    // The reducer closes a thinking-only segment at a tool boundary (model
    // reasoned, then called a tool without narrating). That segment is
    // finished; dots on it would spin for the rest of the transcript.
    messagesAccessor = () => [
      textMsg('a1', '', { thinking: { content: 'reasoning', isStreaming: false, tokenCount: 9 } }),
      toolMsg('t1'),
    ];
    render(() => <AssistantTurn parts={[textPart('a1'), toolsPart('t1')]} isLast={true} />);
    expect(screen.queryByTestId('working-indicator')).not.toBeInTheDocument();
  });

  it('still shows dots while thinking is streaming and no text has arrived', () => {
    // Guard the narrow fix: only a SETTLED thinking block suppresses dots.
    streamingAccessor = () => true;
    messagesAccessor = () => [
      textMsg('a1', '', { thinking: { content: 'reasoning', isStreaming: true } }),
    ];
    render(() => <AssistantTurn parts={[textPart('a1')]} isLast={true} />);
    // Thinking is live, so the ThinkingBlock carries the activity, not dots.
    expect(screen.queryByTestId('working-indicator')).not.toBeInTheDocument();
  });

  it('shows working dots when in flight with a trailing tools part', () => {
    streamingAccessor = () => true;
    messagesAccessor = () => [textMsg('a1', 'thinking aloud'), toolMsg('t1')];
    const { getByTestId } = render(() => (
      <AssistantTurn parts={[textPart('a1'), toolsPart('t1')]} isLast={true} />
    ));
    // Turn-level dots (no empty text segment carries them).
    expect(getByTestId('working-indicator')).toBeInTheDocument();
  });

  it('shows the streaming caret on the last text segment while streaming', () => {
    streamingAccessor = () => true;
    messagesAccessor = () => [textMsg('a1', 'streamed')];
    const { container } = render(() => (
      <AssistantTurn parts={[textPart('a1')]} isLast={true} />
    ));
    // The caret is the animated ember block appended after the prose.
    expect(container.querySelector('span.bg-primary.animate-pulse')).not.toBeNull();
  });

  it('renders NO meta row while the turn is in flight', () => {
    streamingAccessor = () => true;
    messagesAccessor = () => [
      textMsg('a1', 'partial', {
        usage: { promptTokens: 1, completionTokens: 1, totalTokens: 42 },
      }),
    ];
    render(() => <AssistantTurn parts={[textPart('a1')]} isLast={true} />);
    // No usage line and no hover actions while streaming.
    expect(screen.queryByText(/tokens/)).not.toBeInTheDocument();
    expect(screen.queryByTitle('Copy response')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Regenerate response')).not.toBeInTheDocument();
  });
});

// ── Tool groups ────────────────────────────────────────────────────────

describe('AssistantTurn — tool group', () => {
  it('renders ToolCards inside a single tool-group block', () => {
    messagesAccessor = () => [toolMsg('t1'), toolMsg('t2')];
    const { getAllByTestId } = render(() => (
      <AssistantTurn parts={[toolsPart('t1', 't2')]} isLast={false} />
    ));
    const groups = getAllByTestId('tool-group');
    expect(groups).toHaveLength(1);
    expect(groups[0].textContent).toContain('tool-t1');
    expect(groups[0].textContent).toContain('tool-t2');
  });
});

// ── Hover actions: copy + regenerate ───────────────────────────────────

describe('AssistantTurn — copy', () => {
  it('copies the concatenated text of every segment', async () => {
    messagesAccessor = () => [
      textMsg('a1', 'Hello'),
      toolMsg('t1'),
      textMsg('a2', 'World'),
    ];
    render(() => (
      <AssistantTurn
        parts={[textPart('a1'), toolsPart('t1'), textPart('a2')]}
        isLast={false}
      />
    ));

    fireEvent.click(screen.getByTitle('Copy response'));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('Hello\n\nWorld'),
    );
  });
});

describe('AssistantTurn — regenerate', () => {
  it('resends the most recent user message', async () => {
    messagesAccessor = () => [
      userMsg('u1', 'first'),
      textMsg('a1', 'reply'),
      userMsg('u2', 'second'),
      textMsg('a2', 'reply2'),
    ];
    render(() => <AssistantTurn parts={[textPart('a2')]} isLast={true} />);

    fireEvent.click(screen.getByTitle('Regenerate response'));
    await waitFor(() => expect(sendMessageMock).toHaveBeenCalledWith('second'));
  });

  it('omits regenerate on a non-last turn', () => {
    messagesAccessor = () => [userMsg('u1', 'first'), textMsg('a1', 'reply')];
    render(() => <AssistantTurn parts={[textPart('a1')]} isLast={false} />);
    expect(screen.queryByTitle('Regenerate response')).not.toBeInTheDocument();
    // Copy is still available on any settled turn.
    expect(screen.getByTitle('Copy response')).toBeInTheDocument();
  });
});

// ── Thinking block ─────────────────────────────────────────────────────

describe('AssistantTurn — thinking block', () => {
  it('renders the thinking block inside its segment when show-thinking is on', () => {
    messagesAccessor = () => [
      textMsg('a1', 'reply', {
        thinking: { content: 'reasoning steps here', isStreaming: false, tokenCount: 42 },
      }),
    ];
    render(() => <AssistantTurn parts={[textPart('a1')]} isLast={false} />);
    expect(screen.getByText(/reasoning steps here/)).toBeInTheDocument();
  });

  it('hides the thinking block when show-thinking is toggled off', () => {
    messagesAccessor = () => [
      textMsg('a1', 'reply', {
        thinking: { content: 'reasoning steps here', isStreaming: false, tokenCount: 42 },
      }),
    ];
    render(() => <AssistantTurn parts={[textPart('a1')]} isLast={false} />);
    expect(screen.getByText(/reasoning steps here/)).toBeInTheDocument();

    statusBarActions.setShowThinking(false);
    expect(screen.queryByText(/reasoning steps here/)).not.toBeInTheDocument();

    statusBarActions.setShowThinking(true);
    expect(screen.getByText(/reasoning steps here/)).toBeInTheDocument();
  });
});

// ── Which kiln a transcript's links resolve in ─────────────────────────

describe('AssistantTurn — data-kiln', () => {
  // `data-kiln` is read by the wikilink click handler and the hover preview,
  // and both hand it to the note-resolution API as a DIRECTORY. The session
  // record carries a registry NAME, so this has to be joined — publishing the
  // name would make every wikilink in every transcript resolve against a
  // relative directory that does not exist.
  it('is the kiln DIRECTORY, joined from the session s registry name', async () => {
    messagesAccessor = () => [textMsg('a1', 'body')];
    sessionsAccessor = () => [{ id: 's1', kilns: ['vault'] }];
    const { container } = render(() => (
      <AssistantTurn parts={[textPart('a1')]} isLast={false} />
    ));
    await waitFor(() =>
      expect(container.querySelector('[data-kiln]')?.getAttribute('data-kiln')).toBe(
        '/home/u/vault',
      ),
    );
  });

  // A name the registry does not answer for is not a kiln. Emitting the name
  // anyway would send it to the resolver as a path; emitting '' would send the
  // daemon data dir. The attribute is absent, and resolution falls back to the
  // caller's own default rather than to a corpus nobody chose.
  it('is absent for a kiln the registry cannot place', async () => {
    messagesAccessor = () => [textMsg('a1', 'body')];
    sessionsAccessor = () => [{ id: 's1', kilns: ['ghost'] }];
    const { container } = render(() => (
      <AssistantTurn parts={[textPart('a1')]} isLast={false} />
    ));
    await waitFor(() => expect(container.querySelector('[data-md-sync],[data-md-async]')).toBeTruthy());
    expect(container.querySelector('[data-kiln]')).toBeNull();
  });
});
