import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { ChatInput } from '../ChatInput';

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
    chatMode: () => 'normal',
    setChatMode: mockSetChatMode,
    switchMode: mockSwitchMode,
    sessionId: () => 'test-session',
    addSystemMessage: mockAddSystemMessage,
    clearMessages: mockClearMessages,
    activeTools: () => [],
    subagentEvents: () => [],
    pendingInteraction: () => null,
    respondToInteraction: vi.fn(),
  }),
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({
      id: 'test-session',
      state: 'active',
      kilns: ['/tmp/test-kiln'],
      // `null` is the daemon's "floating" (no-workspace) state. It used to be
      // spelled `workspace == kilns[0]`, which this side had to re-derive.
      workspace: null,
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
  nextChatMode: (mode: string) => (mode === 'normal' ? 'plan' : 'normal'),
}));

vi.mock('../AutocompletePopup', () => ({
  AutocompletePopup: () => <div data-testid="autocomplete-popup-mock" />,
}));

vi.mock('@/lib/api', () => ({
  // Mock must match CommandResult (api.ts): { result, type }. The daemon's
  // CommandResponse (web/routes/session_commands.rs) always sets `type` to
  // "success" | "error"; a successful command returns "success".
  executeCommand: vi.fn(async () => ({ result: 'Command executed', type: 'success' })),
  // SessionScopeChips (rendered inside ChatInput) loads these on mount.
  listKilns: vi.fn(async () => []),
  listProjects: vi.fn(async () => []),
  connectSessionKiln: vi.fn(),
  disconnectSessionKiln: vi.fn(),
  setSessionWorkspace: vi.fn(),
  // SessionStatusChips (also rendered inside ChatInput): no plugin slots, no
  // review policy, and a review event stream that never emits.
  getSessionStatus: vi.fn(async () => []),
  listModes: vi.fn(async () => ({ current_mode_id: 'normal', modes: [] })),
  subscribeToEvents: vi.fn(() => () => {}),
}));

vi.mock('@/lib/review-api', () => ({
  listReviewHunks: vi.fn(async () => ({ session_id: 'test-session', hunks: [], comments: [] })),
}));

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

describe('ChatInput — session context chips', () => {
  it('shows the kiln chip for the current session', () => {
    render(() => <ChatInput />);
    const chips = screen.getByTestId('context-chips');
    expect(chips).toBeInTheDocument();
    expect(chips.textContent).toContain('test-kiln');
  });

  it('floating session (workspace: null) reads "Session folder" for the project chip', () => {
    render(() => <ChatInput />);
    expect(screen.getByTestId('scope-project').textContent).toContain('Session folder');
  });
});
