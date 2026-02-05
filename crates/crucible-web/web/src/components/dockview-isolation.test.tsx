import { render, screen } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';
import { ChatInput } from './ChatInput';
import { MessageList } from './MessageList';
import { ChatContent } from './ChatContent';

vi.mock('@/lib/api', () => ({
  sendChatMessage: vi.fn(),
  subscribeToEvents: vi.fn(() => () => {}),
  respondToInteraction: vi.fn(),
  cancelSession: vi.fn(),
  generateMessageId: () => `msg_${Date.now()}_test`,
  listSessions: vi.fn(() => Promise.resolve([])),
  listModels: vi.fn(() => Promise.resolve([])),
  switchModel: vi.fn(),
  createSession: vi.fn(),
}));

vi.mock('@/hooks/useMediaRecorder', () => ({
  useMediaRecorder: () => ({
    isRecording: () => false,
    audioLevel: () => 0,
    startRecording: vi.fn(),
    stopRecording: vi.fn(() => Promise.resolve(new Blob())),
  }),
}));

describe('Dockview Panel Isolation', () => {
  describe('ChatInput', () => {
    it('renders without providers (simulates dockview panel)', () => {
      expect(() => render(() => <ChatInput />)).not.toThrow();
    });

    it('shows disabled state when no session context', () => {
      render(() => <ChatInput />);
      const textarea = screen.getByTestId('chat-input');
      expect(textarea).toBeDisabled();
    });

    it('shows placeholder indicating no session', () => {
      render(() => <ChatInput />);
      const textarea = screen.getByTestId('chat-input');
      expect(textarea.getAttribute('placeholder')).toBe('Select a session first...');
    });
  });

  describe('MessageList', () => {
    it('renders without providers (simulates dockview panel)', () => {
      expect(() => render(() => <MessageList />)).not.toThrow();
    });

    it('shows empty state message', () => {
      render(() => <MessageList />);
      expect(screen.getByText('Select or create a session to start chatting')).toBeInTheDocument();
    });
  });

  describe('ChatContent', () => {
    it('renders without providers (simulates dockview panel)', () => {
      expect(() => render(() => <ChatContent />)).not.toThrow();
    });
  });
});
