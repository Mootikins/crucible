import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { ChatContent } from '../ChatContent';

// Mock the child components
vi.mock('../MessageList', () => ({
  MessageList: () => <div data-testid="message-list">Message List</div>,
}));

vi.mock('../ChatInput', () => ({
  ChatInput: () => <div data-testid="chat-input">Chat Input</div>,
}));

vi.mock('../ToolCard', () => ({
  ToolCard: (props: any) => (
    <div data-testid={`tool-card-${props.toolCall.id}`}>Tool: {props.toolCall.name}</div>
  ),
}));

vi.mock('../SubagentCard', () => ({
  SubagentCard: (props: any) => (
    <div data-testid={`subagent-card-${props.event.id}`}>Subagent Event</div>
  ),
}));

vi.mock('../DelegationCard', () => ({
  DelegationCard: (props: any) => (
    <div data-testid={`delegation-card-${props.event.id}`}>Delegation Event</div>
  ),
}));

vi.mock('./interactions', () => ({
  InteractionHandler: (props: any) => (
    <div data-testid={`interaction-handler-${props.request.id}`}>
      Interaction: {props.request.type}
    </div>
  ),
}));

// Mock the context
const mockRespondToInteraction = vi.fn();

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    activeTools: () => [],
    subagentEvents: () => [],
    pendingInteraction: () => null,
    respondToInteraction: mockRespondToInteraction,
  }),
}));

describe('ChatContent', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders MessageList component', () => {
    render(() => <ChatContent />);
    const messageList = screen.getByTestId('message-list');
    expect(messageList).toBeInTheDocument();
  });

  it('renders ChatInput component', () => {
    render(() => <ChatContent />);
    const chatInput = screen.getByTestId('chat-input');
    expect(chatInput).toBeInTheDocument();
  });

  it('does not render tool cards when no active tools', () => {
    render(() => <ChatContent />);
    const toolCards = screen.queryAllByTestId(/^tool-card-/);
    expect(toolCards).toHaveLength(0);
  });

  it('does not render subagent cards when no subagent events', () => {
    render(() => <ChatContent />);
    const subagentCards = screen.queryAllByTestId(/^subagent-card-/);
    expect(subagentCards).toHaveLength(0);
  });

  it('does not render interaction handler when no pending interaction', () => {
    render(() => <ChatContent />);
    const handlers = screen.queryAllByTestId(/^interaction-handler-/);
    expect(handlers).toHaveLength(0);
  });

  it('has correct layout structure with flex column', () => {
    const { container } = render(() => <ChatContent />);
    const mainDiv = container.querySelector('[data-message-renderer="markdown-it"]');
    expect(mainDiv).toHaveClass('h-full', 'flex', 'flex-col', 'overflow-hidden');
  });

  it('renders main container with correct data attribute', () => {
    const { container } = render(() => <ChatContent />);
    const mainDiv = container.querySelector('[data-message-renderer="markdown-it"]');
    expect(mainDiv).toBeInTheDocument();
  });

  it('renders message list in flex container', () => {
    const { container } = render(() => <ChatContent />);
    const flexContainer = container.querySelector('.flex-1.min-h-0.flex.flex-col');
    expect(flexContainer).toBeInTheDocument();
    expect(screen.getByTestId('message-list')).toBeInTheDocument();
  });

  it('renders chat input at bottom of layout', () => {
    const { container } = render(() => <ChatContent />);
    const form = screen.getByTestId('chat-input');
    expect(form).toBeInTheDocument();
    // ChatInput should be rendered after message list
    const messageList = screen.getByTestId('message-list');
    expect(messageList).toBeInTheDocument();
  });

  it('renders with correct overflow handling', () => {
    const { container } = render(() => <ChatContent />);
    const mainDiv = container.querySelector('[data-message-renderer="markdown-it"]');
    expect(mainDiv).toHaveClass('overflow-hidden');
  });

  it('renders message list with correct flex properties', () => {
    const { container } = render(() => <ChatContent />);
    const flexContainer = container.querySelector('.flex-1.min-h-0.flex.flex-col');
    expect(flexContainer).toHaveClass('flex-1', 'min-h-0', 'flex', 'flex-col');
  });

  it('renders all core components together', () => {
    render(() => <ChatContent />);
    expect(screen.getByTestId('message-list')).toBeInTheDocument();
    expect(screen.getByTestId('chat-input')).toBeInTheDocument();
  });

  it('renders without tool cards section when no tools', () => {
    const { container } = render(() => <ChatContent />);
    const toolSections = container.querySelectorAll('.px-4.py-2.border-t');
    // Should not have tool section when no tools
    expect(toolSections.length).toBe(0);
  });
});
