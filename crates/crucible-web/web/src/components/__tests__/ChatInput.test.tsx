import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@solidjs/testing-library';
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
const mockClearMessages = vi.fn();
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
    clearMessages: mockClearMessages,
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

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({
      session_id: 'test-session',
      state: 'active',
      kilns: ['/tmp/test-kiln'],
      workspace,
      agent_model: 'test-model',
    }),
    cancelCurrentOperation: mockCancelCurrentOperation,
    availableModels: () => ['model-1', 'model-2'],
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
  useAutocomplete: () => ({
    isOpen: () => false,
    items: () => [],
    selectedIndex: () => -1,
    onInput: vi.fn(),
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

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  // The docked permission card reads the file it is about to overwrite.
  getFileContent: vi.fn(async () => ''),
  // SessionScopeChips (rendered inside ChatInput) loads this on mount. Its
  // kiln roster is NOT stubbed here: it arrives over the fetch below.
  listProjects: vi.fn(async () => []),
  connectSessionKiln: vi.fn(),
  disconnectSessionKiln: vi.fn(),
  setSessionWorkspace: vi.fn(),
  // SessionStatusChips (also rendered inside ChatInput): no plugin slots, no
  // review policy, and a review event stream that never emits.
  getSessionStatus: vi.fn(async () => []),
  listModes: vi.fn(async () => ({ current_mode_id: 'ask', modes: [] })),
  subscribeToEvents: vi.fn(() => () => {}),
}));

vi.mock('@/lib/review-api', () => ({
  listReviewHunks: vi.fn(async () => ({ session_id: 'test-session', hunks: [], comments: [] })),
}));

// The roster the scope chips read through the shared kiln query.
let kilnEnv: TestQueryEnv;

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  kilnEnv = createTestQueryEnv({ 'GET /api/kilns': () => ({ kilns: [] }) });
});

afterEach(() => {
  cleanup();
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
    for (const id of ['model-picker-button', 'chat-mode-control-mock', 'scope-project', 'scope-kiln']) {
      expect(surface().contains(screen.getByTestId(id)), `${id} is inside the capsule`).toBe(false);
    }
  });

  // PRIORITY produces this order, not the order `liveChips` lists them in:
  // the scope chips come from a hook that states 30 and 40, so a model or a
  // mode without a priority of its own would sort behind both of them.
  it('draws the shared chip row BELOW the capsule: model, mode, then the scope', () => {
    render(() => <ChatInput />);
    const row = screen.getByTestId('composer-chip-row');
    expect(surface().compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const ids = Array.from(row.querySelectorAll('[data-testid]')).map((e) =>
      e.getAttribute('data-testid'),
    );
    expect(ids.indexOf('model-picker-button')).toBeLessThan(ids.indexOf('chat-mode-control-mock'));
    expect(ids.indexOf('chat-mode-control-mock')).toBeLessThan(ids.indexOf('scope-project'));
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
    action_type: 'bash',
    tokens: ['rm', '-rf', 'build'],
  });

  it('draws no card while nothing is pending', () => {
    render(() => <ChatInput />);
    expect(screen.queryByTestId('composer-dock')).toBeNull();
    expect(
      (document.querySelector('.composer-surface') as HTMLElement).getAttribute('data-docked'),
    ).toBeNull();
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

  it('squares the prompt\'s top edge while the card is docked', () => {
    setPending(permission());
    render(() => <ChatInput />);
    const surface = document.querySelector('.composer-surface') as HTMLElement;
    expect(surface.getAttribute('data-docked')).toBe('true');
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
