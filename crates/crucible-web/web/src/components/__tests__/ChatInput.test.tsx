import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen } from '@solidjs/testing-library';
import { ChatInput } from '../ChatInput';

// Mock the contexts
const mockSendMessage = vi.fn();
const mockCancelStream = vi.fn();
const mockCancelCurrentOperation = vi.fn();
const mockSetChatMode = vi.fn();
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
    chatMode: () => 'normal',
    setChatMode: mockSetChatMode,
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
      kiln: '/tmp/test-kiln',
      agent_model: 'test-model',
    }),
    cancelCurrentOperation: mockCancelCurrentOperation,
    availableModels: () => ['model-1', 'model-2'],
    switchModel: mockSwitchModel,
    refreshModels: mockRefreshModels,
    selectedProvider: () => ({ provider_type: 'ollama' }),
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
  executeCommand: vi.fn(async () => ({ result: 'Command executed' })),
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

  it('displays current model in picker button', () => {
    render(() => <ChatInput />);
    const modelButton = screen.getByTestId('model-picker-button');
    expect(modelButton.textContent).toContain('ollama/test-model');
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

  it('has correct form structure with border and padding', () => {
    const { container } = render(() => <ChatInput />);
    const form = screen.getByTestId('chat-input-form');
    expect(form).toHaveClass('border-t', 'border-neutral-800', 'p-4');
  });

  it('renders textarea with correct classes', () => {
    render(() => <ChatInput />);
    const textarea = screen.getByTestId('chat-input');
    expect(textarea).toHaveClass('flex-1', 'w-full', 'bg-transparent', 'text-neutral-100');
  });
});
