import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@solidjs/testing-library';
import { composerComments } from '@/stores/composerComments';
import type { DiffsetSource } from '@/lib/diffset';
import { installFakeEventSource } from '@/test-utils/sse';
import { createSignal } from 'solid-js';
import type { InteractionRequest } from '@/lib/types';
import { ChatInput } from '../ChatInput';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

/** The request the composer is parked on, or none. */
const [pending, setPending] = createSignal<InteractionRequest | null>(null);

// Mock the contexts
const mockSendMessage = vi.fn();
const mockCancelStream = vi.fn();
const mockCancelCurrentOperation = vi.fn();
const mockSetChatMode = vi.fn();
const mockSwitchMode = vi.fn();
const mockAddSystemMessage = vi.fn();
const mockSwitchModel = vi.fn();
const mockRefreshModels = vi.fn();

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    sendMessage: mockSendMessage,
    isLoading: () => false,
    isStreaming: () => false,
    cancelStream: mockCancelStream,
    error: () => null,
    connectionStatus: () => 'connected',
    retryConnection: vi.fn(),
    chatMode: () => 'ask',
    setChatMode: mockSetChatMode,
    switchMode: mockSwitchMode,
    sessionId: () => 'test-session',
    addSystemMessage: mockAddSystemMessage,
    activeTools: () => [],
    subagentEvents: () => [],
    pendingInteraction: pending,
    respondToInteraction: vi.fn(),
  }),
}));

// `null` is the daemon's "floating" (no-workspace) state. It used to be
// spelled `workspace == kilns[0]`, which this side had to re-derive. A test
// that needs the session's OWN scratch folder sets this instead.
let workspace: string | null = null;
let modelOptions = ['model-1', 'model-2'];
let modelName = 'test-model';
let agentDetail: Record<string, unknown> | null = null;

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({
      session_id: 'test-session',
      state: 'active',
      kilns: ['/tmp/test-kiln'],
      workspace,
      agent_model: modelName,
    }),
    cancelCurrentOperation: mockCancelCurrentOperation,
    availableModels: () => modelOptions,
    switchModel: mockSwitchModel,
    refreshModels: mockRefreshModels,
    selectedProvider: () => ({ provider_type: 'ollama' }),
    applySessionScope: vi.fn(),
  }),
}));

vi.mock('@/hooks/useMediaRecorder', () => ({
  useMediaRecorder: () => ({
    isRecording: () => false,
    audioLevel: () => 0,
    startRecording: vi.fn(),
    stopRecording: vi.fn(),
  }),
}));

vi.mock('@/hooks/useAutocomplete', () => ({
  useAutocomplete: (opts: { setInput: (v: string) => void }) => ({
    isOpen: () => false,
    items: () => [],
    selectedIndex: () => -1,
    onInput: (e: InputEvent) => opts.setInput((e.currentTarget as HTMLTextAreaElement).value),
    onKeyDown: vi.fn(),
    complete: vi.fn(),
  }),
}));

vi.mock('../MicButton', () => ({
  MicButton: () => <div data-testid="mic-button-mock" />,
}));

vi.mock('../ChatModeControl', () => ({
  ChatModeControl: () => <div data-testid="chat-mode-control-mock" />,
  nextChatMode: (mode: string) => (mode === 'ask' ? 'plan' : 'ask'),
}));

vi.mock('../AutocompletePopup', () => ({
  AutocompletePopup: () => <div data-testid="autocomplete-popup-mock" />,
}));

// No `vi.mock('@/lib/api')`. Everything the composer's chips read arrives
// over the routes in `beforeEach` — the scope chips' project roster, the
// status chips' mode list and status slots, and the file the docked
// permission card is about to overwrite.

// The roster the scope chips read through the shared kiln query.
let kilnEnv: TestQueryEnv;
/** The daemon refuses the delete of the chip. */
let deleteFails = false;

beforeEach(() => {
  deleteFails = false;
  modelOptions = ['model-1', 'model-2'];
  modelName = 'test-model';
  agentDetail = null;
  localStorage.clear();
  resetKilnsForTests();
  installFakeEventSource();
  kilnEnv = createTestQueryEnv({
    'POST /api/rpc/session.get': () => ({ session_id: 'test-session', agent_model: modelName, agent: agentDetail }),
    'POST /api/rpc/kiln.list': () => [],
    'POST /api/rpc/project.list': () => [],
    'GET /api/session/test-session/modes': () => ({ current_mode_id: 'ask', modes: [] }),
    'GET /api/session/test-session/status': () => ({ status: [] }),
    'POST /api/session/test-session/command': () => ({ result: 'Context cleared', type: 'success' }),
    // The docked permission card reads the file it is about to overwrite.
    'GET /api/kiln/file': () => ({ content: '' }),
    // The `×` of a comment chip deletes the comment.
    'POST /api/rpc/diff.delete_comment': () =>
      deleteFails
        ? new Response(
            JSON.stringify({ error: { code: 422, message: 'the diffset has no such comment' } }),
            { status: 422, headers: { 'Content-Type': 'application/json' } },
          )
        : { diffset: 'session-test-session', comment_id: 'c1', deleted: true },
  });
});

afterEach(() => {
  cleanup();
  composerComments.resetForTests();
  kilnEnv.restore();
  resetKilnsForTests();
  setPending(null);
  workspace = null;
});
describe('ChatInput', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSendMessage.mockResolvedValue(undefined);
  });

  it('renders textarea element', () => {
    render(() => <ChatInput />);
    const textarea = screen.getByTestId('chat-input');
    expect(textarea).toBeInTheDocument();
  });

  it('shows placeholder text when session is active', () => {
    render(() => <ChatInput />);
    const textarea = screen.getByTestId('chat-input') as HTMLTextAreaElement;
    expect(textarea.placeholder).toBe('Type a message...');
  });

  it('textarea is not disabled when session is active', () => {
    render(() => <ChatInput />);
    const textarea = screen.getByTestId('chat-input') as HTMLTextAreaElement;
    expect(textarea.disabled).toBe(false);
  });

  it('renders send button', () => {
    render(() => <ChatInput />);
    const sendButton = screen.getByTestId('send-button');
    expect(sendButton).toBeInTheDocument();
  });

  it('a comment of the diff pane becomes a chip, and rides the next message', async () => {
    const source: DiffsetSource = { kind: 'branch', root: '/repo', base: '', head: null };
    composerComments.attach('test-session', {
      id: 'c1',
      source,
      label: 'a.rs L1–2',
      title: 'src/a.rs · why this?',
    });
    composerComments.attach('other-session', {
      id: 'c2',
      source,
      label: 'b.rs L4',
      title: 'src/b.rs · and this?',
    });
    render(() => <ChatInput />);

    // Only the chips of this session, and the send is live with no text.
    const chips = () => screen.queryAllByTestId('composer-attachment');
    await waitFor(() => expect(chips().map((c) => c.textContent)).toEqual(['a.rs L1–2']));
    expect((screen.getByTestId('send-button') as HTMLButtonElement).disabled).toBe(false);

    // The completion hook owns the text of the field, and this file mocks
    // it. The message therefore goes with its chips and no text, which is a
    // message the composer must send.
    fireEvent.submit(screen.getByTestId('chat-input-form'));

    // The message carries the reference, not the text of the comment.
    await waitFor(() => expect(mockSendMessage.mock.calls).toEqual([['', [{ id: 'c1', source }]]]));
    await waitFor(() => expect(chips()).toHaveLength(0));
    expect(composerComments.of('other-session')).toHaveLength(1);
  });

  // The daemon clears the model context and sends `context_cleared`, which
  // draws the divider. The transcript keeps the history above it.
  it('/clear clears through the daemon and keeps the transcript', async () => {
    render(() => <ChatInput />);
    fireEvent.input(screen.getByTestId('chat-input'), { target: { value: '/clear' } });
    fireEvent.submit(screen.getByTestId('chat-input-form'));
    await waitFor(() => expect(mockAddSystemMessage).toHaveBeenCalledWith('Context cleared'));
    expect((await kilnEnv.fetch.sent(kilnEnv.fetch.mock.calls.length - 1)).body).toEqual({ command: '/clear' });
  });

  // Only a built-in command runs on the command route. The daemon routes any
  // other `/name` from the session's catalog, so the composer sends it as a
  // message.
  it('a command that is not built in goes to the daemon as a message', async () => {
    render(() => <ChatInput />);
    fireEvent.input(screen.getByTestId('chat-input'), { target: { value: '/reflect last turn' } });
    fireEvent.submit(screen.getByTestId('chat-input-form'));
    await waitFor(() => expect(mockSendMessage).toHaveBeenCalledWith('/reflect last turn', []));
    expect(kilnEnv.fetch.calls('POST /api/session/test-session/command')).toBe(0);
  });

  it('removing a chip deletes the stored comment', async () => {
    const source: DiffsetSource = { kind: 'session_record', session: 'test-session' };
    composerComments.attach('test-session', {
      id: 'c1',
      source,
      label: 'a.rs L1',
      title: 'src/a.rs · why this?',
    });
    render(() => <ChatInput />);

    const remove = await screen.findByTestId('composer-attachment-remove');
    expect(remove.getAttribute('aria-label')).toBe('Remove a.rs L1');
    fireEvent.click(remove);

    await waitFor(() => expect(screen.queryAllByTestId('composer-attachment')).toHaveLength(0));
    expect(composerComments.of('test-session')).toEqual([]);
    // The chip and the comment are one thing: the comment leaves the store.
    await waitFor(() => expect(kilnEnv.fetch.calls('POST /api/rpc/diff.delete_comment')).toBe(1));
    const sent = await kilnEnv.fetch.sent(kilnEnv.fetch.mock.calls.length - 1);
    expect(sent.body).toEqual({ source, comment_id: 'c1' });
    expect(mockSendMessage).not.toHaveBeenCalled();
  });

  it('a refused delete puts the chip back', async () => {
    deleteFails = true;
    composerComments.attach('test-session', {
      id: 'c1',
      source: { kind: 'session_record', session: 'test-session' },
      label: 'a.rs L1',
      title: 'src/a.rs · why this?',
    });
    render(() => <ChatInput />);

    fireEvent.click(await screen.findByTestId('composer-attachment-remove'));

    // The pane still shows the comment, so the composer must show its chip.
    await waitFor(() => expect(composerComments.of('test-session')).toHaveLength(1));
    expect(screen.queryAllByTestId('composer-attachment')).toHaveLength(1);
  });

  it('a comment that a message already took is never deleted', async () => {
    const source: DiffsetSource = { kind: 'session_record', session: 'test-session' };
    const chip = { id: 'c1', source, label: 'a.rs L1', title: 'src/a.rs · why this?' };
    composerComments.attach('test-session', chip);
    render(() => <ChatInput />);

    // The message carries the comment. The agent now has it.
    fireEvent.submit(screen.getByTestId('chat-input-form'));
    await waitFor(() => expect(screen.queryAllByTestId('composer-attachment')).toHaveLength(0));

    // The pane attaches it again. The `×` now only drops the chip.
    composerComments.attach('test-session', chip);
    fireEvent.click(await screen.findByTestId('composer-attachment-remove'));

    await waitFor(() => expect(screen.queryAllByTestId('composer-attachment')).toHaveLength(0));
    expect(kilnEnv.fetch.calls('POST /api/rpc/diff.delete_comment')).toBe(0);
  });

  it('disables send button when input is empty', () => {
    render(() => <ChatInput />);
    const sendButton = screen.getByTestId('send-button') as HTMLButtonElement;
    expect(sendButton.disabled).toBe(true);
  });

  it('renders model picker button', () => {
    render(() => <ChatInput />);
    const modelButton = screen.getByTestId('model-picker-button');
    expect(modelButton).toBeInTheDocument();
  });

  it('displays the model id as-is (no provider-type prefix)', () => {
    render(() => <ChatInput />);
    const modelButton = screen.getByTestId('model-picker-button');
    // The picker is scoped to the session's provider, so the model shows
    // unprefixed — not "openai/…"/"ollama/…" (that misled openai-compatible
    // endpoints into always reading "openai/").
    expect(modelButton.textContent).toContain('test-model');
    expect(modelButton.textContent).not.toContain('ollama/');
    expect(modelButton.textContent).not.toContain('openai/');
  });

  it('renders form with correct data-testid', () => {
    render(() => <ChatInput />);
    const form = screen.getByTestId('chat-input-form');
    expect(form).toBeInTheDocument();
  });

  it('renders mic button mock', () => {
    render(() => <ChatInput />);
    const micButton = screen.getByTestId('mic-button-mock');
    expect(micButton).toBeInTheDocument();
  });

  it('renders chat mode control mock', () => {
    render(() => <ChatInput />);
    const chatModeControl = screen.getByTestId('chat-mode-control-mock');
    expect(chatModeControl).toBeInTheDocument();
  });

  // These two replace a pair of assertions that named the exact class list
  // the markup happened to carry ('border-t border-hairline p-3'). That kind
  // of gate re-states the implementation instead of constraining it: it fails
  // on any restyle, passes on any restyle that keeps the string, and tells a
  // reader nothing about what must stay true. Both now name a DECISION.

  it('draws no rule between the transcript and the composer', () => {
    render(() => <ChatInput />);
    const form = screen.getByTestId('chat-input-form');
    // The transcript fades into this strip (`.transcript-fade`); a border
    // here would box the composer in and re-draw the hard edge that fade
    // exists to remove.
    for (const cls of Array.from(form.classList)) {
      expect(cls.startsWith('border-t')).toBe(false);
    }
  });

  it('holds the composer to the same measure as the transcript', () => {
    render(() => <ChatInput />);
    const form = screen.getByTestId('chat-input-form');
    // The form is full-bleed; an inner wrapper centres on --chat-measure, the
    // SAME token MessageList uses. If the two ever stop agreeing, the
    // composer's edges stop lining up under the transcript's.
    const measured = form.querySelector('.max-w-\\[var\\(--chat-measure\\)\\]');
    expect(measured).not.toBeNull();
    expect(measured!.classList).toContain('mx-auto');
  });

  it('gives the prompt no surface of its own', () => {
    render(() => <ChatInput />);
    const textarea = screen.getByTestId('chat-input');
    // The card is the field; the textarea is a hole in it. A background or a
    // border here would draw a second box inside the first.
    expect(textarea.classList).toContain('bg-transparent');
    // And it must NOT draw its own focus ring — the card does, and two ember
    // treatments 2px apart is the defect index.css's focus note records.
    expect(textarea.classList).not.toContain('focus-ring');
    expect(textarea.classList).toContain('outline-none');
  });
});

describe('ChatInput — the prompt carries only the message', () => {
  /** The bordered prompt surface. */
  const surface = () => document.querySelector('.composer-surface') as HTMLElement;

  it('keeps the mic in the prompt and every chip out of it', () => {
    render(() => <ChatInput />);
    expect(surface().contains(screen.getByTestId('mic-button-mock'))).toBe(true);
    for (const id of [
      'model-picker-button',
      'chat-mode-control-mock',
      'scope-project',
      'scope-kiln',
    ]) {
      expect(surface().contains(screen.getByTestId(id)), `${id} is inside the capsule`).toBe(false);
    }
  });

  // PRIORITY produces this order, not the order `liveChips` lists them in:
  // the scope chips come from a hook that states 30 and 40, so a model or a
  // mode without a priority of its own would sort behind both of them.
  it('draws the shared chip row BELOW the capsule: mode, model, then the scope', () => {
    render(() => <ChatInput />);
    const row = screen.getByTestId('composer-chip-row');
    expect(surface().compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const ids = Array.from(row.querySelectorAll('[data-testid]')).map((e) =>
      e.getAttribute('data-testid'),
    );
    expect(ids.indexOf('chat-mode-control-mock')).toBeLessThan(ids.indexOf('model-picker-button'));
    expect(ids.indexOf('model-picker-button')).toBeLessThan(ids.indexOf('scope-project'));
    expect(ids.indexOf('scope-project')).toBeLessThan(ids.indexOf('scope-kiln'));
  });

  it('shows the model value without an axis label', () => {
    render(() => <ChatInput />);
    const text = screen.getByTestId('model-picker-button').textContent ?? '';
    expect(text).toContain('test-model');
    expect(text).not.toContain('Model ·');
  });
});

describe('ChatInput — a pending request docks on the prompt', () => {
  const permission = (): InteractionRequest => ({
    kind: 'permission',
    id: 'perm-1',
    action: { type: 'bash', tokens: ['rm', '-rf', 'build'] },
  });

  it('draws no card while nothing is pending', () => {
    render(() => <ChatInput />);
    expect(screen.queryByTestId('composer-dock')).toBeNull();
  });

  it('draws the full card directly above the prompt', () => {
    setPending(permission());
    render(() => <ChatInput />);

    const dock = screen.getByTestId('composer-dock');
    // The whole gate, not a summary: this is where the user answers.
    expect(dock.textContent).toContain('Permission Required');
    expect(screen.getByTestId('perm-allow')).toBeInTheDocument();
    expect(screen.getByTestId('perm-deny')).toBeInTheDocument();

    // Immediately above, with nothing between the two.
    const surface = document.querySelector('.composer-surface') as HTMLElement;
    expect(dock.nextElementSibling?.contains(surface)).toBe(true);
  });
});

describe('ChatInput — session context chips', () => {
  it('shows the kiln chip for the current session on the shared row', () => {
    render(() => <ChatInput />);
    const row = screen.getByTestId('composer-chip-row');
    expect(row.contains(screen.getByTestId('scope-kiln'))).toBe(true);
    expect(screen.getByTestId('scope-kiln').textContent).toContain('test-kiln');
  });

  it('floating session (workspace: null) reads "Session folder" for the project chip', () => {
    render(() => <ChatInput />);
    expect(screen.getByTestId('scope-project').textContent).toContain('Session folder');
  });

  // An ephemeral session DOES have a workspace — its own scratch folder,
  // named after the session id. The chip must not wear that id: it is the
  // longest string on the row and it names nothing the reader can use.
  it('an ephemeral session reads "Session folder" too, keeping the path in the title', () => {
    workspace = '/data/workspaces/test-session';
    render(() => <ChatInput />);
    const chip = screen.getByTestId('scope-project');
    expect(chip.textContent).toContain('Session folder');
    expect(chip.textContent).not.toContain('test-session');
    expect(chip.getAttribute('title')).toBe('/data/workspaces/test-session');
  });
});


it('selects the exact native provider catalogue key after the daemon returns a bare model', async () => {
  modelName = 'vendor/model';
  modelOptions = ['other/vendor/model', 'my-server/vendor/model'];
  agentDetail = { agent_type: 'internal', model: modelName, provider_key: 'my-server', provider: 'openai' };
  render(() => <ChatInput />);
  fireEvent.click(screen.getByTestId('model-picker-button'));
  await waitFor(() => expect(screen.getByTestId('model-option-my-server/vendor/model')).toHaveAttribute('aria-selected', 'true'));
  expect(screen.getByTestId('model-option-other/vendor/model')).toHaveAttribute('aria-selected', 'false');
  expect(screen.getByTestId('model-picker-button')).toHaveTextContent('vendor/model');
  expect(screen.getByTestId('model-picker-button')).not.toHaveTextContent('my-server/');
});

it('keeps ACP model identifiers opaque even when they contain slashes', async () => {
  modelName = 'vendor/model';
  modelOptions = ['vendor/model', 'my-server/vendor/model'];
  agentDetail = { agent_type: 'acp', model: modelName, provider_key: 'my-server' };
  render(() => <ChatInput />);
  fireEvent.click(screen.getByTestId('model-picker-button'));
  await waitFor(() => expect(screen.getByTestId('model-option-vendor/model')).toHaveAttribute('aria-selected', 'true'));
  expect(screen.getByTestId('model-option-my-server/vendor/model')).toHaveAttribute('aria-selected', 'false');
});
