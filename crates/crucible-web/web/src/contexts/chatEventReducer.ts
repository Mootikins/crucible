import { statusBarActions } from '@/stores/statusBarStore';
import type {
  Message,
  ChatEvent,
  InteractionRequest,
  ToolCallDisplay,
  SubagentEvent,
  ContextUsage,
  ChatMode,
} from '@/lib/types';
import { generateMessageId } from '@/lib/api';

type ArraySetter<T> = (value: T[] | ((prev: T[]) => T[])) => void;

interface ChatEventReducerDeps {
  messages: () => Message[];
  currentStreamingMessageId: () => string | null;
  setCurrentStreamingMessageId: (id: string | null) => void;
  firstUserMessage: () => string | null;
  hasReceivedFirstResponse: () => boolean;
  setHasReceivedFirstResponse: (value: boolean) => void;
  onFirstResponse: () => void;
  addMessage: (message: Message) => void;
  updateMessage: (id: string, updates: Partial<Message>) => void;
  appendToMessage: (id: string, content: string) => void;
  setActiveTools: ArraySetter<ToolCallDisplay>;
  setSubagentEvents: ArraySetter<SubagentEvent>;
  setContextUsage: (usage: ContextUsage | null) => void;
  setChatMode: (mode: ChatMode) => void;
  setPendingInteraction: (request: InteractionRequest | null) => void;
  setError: (value: string | null) => void;
  setIsLoading: (value: boolean) => void;
  setIsStreaming: (value: boolean) => void;
}

function updateTool(
  tools: ToolCallDisplay[],
  id: string,
  updates: Partial<ToolCallDisplay>,
): ToolCallDisplay[] {
  return tools.map((tool) => (
    tool.callId === id || tool.id === id ? { ...tool, ...updates } : tool
  ));
}

function upsertSubagentEvent(
  events: SubagentEvent[],
  eventId: string,
  updates: Partial<SubagentEvent>,
  fallback: SubagentEvent,
): SubagentEvent[] {
  const index = events.findIndex((event) => event.id === eventId);
  if (index === -1) {
    return [...events, fallback];
  }

  const next = [...events];
  next[index] = { ...next[index], ...updates };
  return next;
}

export function createChatEventReducer(deps: ChatEventReducerDeps) {
  return (event: ChatEvent) => {
    switch (event.type) {
      case 'token': {
        const messageId = deps.currentStreamingMessageId();
        if (messageId) {
          deps.appendToMessage(messageId, event.content);
        }
        break;
      }

      case 'tool_call':
      case 'tool_call_start': {
        const toolName = 'title' in event ? event.title : ('name' in event ? event.name : '');
        const toolArgs = 'arguments' in event ? JSON.stringify(event.arguments ?? '') : '';
        const tool: ToolCallDisplay = {
          id: event.id,
          name: toolName,
          args: toolArgs,
          status: 'running',
          callId: event.id,
        };
        deps.setActiveTools((prev) => [...prev, tool]);
        break;
      }

      case 'tool_result':
        deps.setActiveTools((prev) => updateTool(prev, event.id, {
          result: event.result ?? '',
          status: 'complete',
        }));
        break;

      case 'tool_result_delta':
        deps.setActiveTools((prev) => prev.map((tool) => {
          if (tool.callId === event.id || tool.id === event.id) {
            return { ...tool, result: (tool.result ?? '') + event.delta };
          }
          return tool;
        }));
        break;

      case 'tool_result_complete':
        deps.setActiveTools((prev) => updateTool(prev, event.id, { status: 'complete' }));
        break;

      case 'tool_result_error':
        deps.setActiveTools((prev) => updateTool(prev, event.id, {
          result: event.error,
          status: 'error',
        }));
        break;

      case 'thinking': {
        const messageId = deps.currentStreamingMessageId();
        if (messageId) {
          const thinkingContent = deps.messages().find((message) => message.id === messageId)?.thinking?.content ?? '';
          deps.updateMessage(messageId, {
            thinking: {
              content: thinkingContent + event.content,
              isStreaming: true,
            },
          });
        }
        break;
      }

      case 'message_complete': {
        const messageId = deps.currentStreamingMessageId();
        const thinkingData = messageId
          ? deps.messages().find((message) => message.id === messageId)?.thinking
          : undefined;
        if (messageId) {
          deps.updateMessage(messageId, {
            id: event.id,
            content: event.content,
            toolCalls: event.tool_calls,
            ...(thinkingData ? {
              thinking: {
                content: thinkingData.content,
                isStreaming: false,
                tokenCount: thinkingData.content.length,
              },
            } : {}),
          });
        }
        deps.setIsStreaming(false);
        deps.setIsLoading(false);
        deps.setActiveTools([]);
        deps.setCurrentStreamingMessageId(null);

        if (!deps.hasReceivedFirstResponse() && deps.firstUserMessage()) {
          deps.setHasReceivedFirstResponse(true);
          deps.onFirstResponse();
        }
        break;
      }

      case 'error': {
        deps.setError(`${event.message} (${event.code})`);
        const messageId = deps.currentStreamingMessageId();
        if (messageId) {
          deps.updateMessage(messageId, {
            content: `Error: ${event.message}`,
          });
        }
        deps.setIsStreaming(false);
        deps.setIsLoading(false);
        deps.setCurrentStreamingMessageId(null);
        break;
      }

      case 'interaction_requested': {
        const { type: _eventType, ...requestData } = event;
        deps.setPendingInteraction(requestData as unknown as InteractionRequest);
        break;
      }

      case 'subagent_spawned':
        deps.setSubagentEvents((prev) => [...prev, {
          id: event.id,
          prompt: event.prompt,
          status: 'spawned',
        }]);
        break;

      case 'subagent_completed':
        deps.setSubagentEvents((prev) => upsertSubagentEvent(
          prev,
          event.id,
          { status: 'completed', summary: event.summary },
          {
            id: event.id,
            prompt: '',
            status: 'completed',
            summary: event.summary,
          },
        ));
        break;

      case 'subagent_failed':
        deps.setSubagentEvents((prev) => upsertSubagentEvent(
          prev,
          event.id,
          { status: 'failed', error: event.error },
          {
            id: event.id,
            prompt: '',
            status: 'failed',
            error: event.error,
          },
        ));
        break;

      case 'delegation_spawned':
        deps.setSubagentEvents((prev) => [...prev, {
          id: event.id,
          prompt: event.prompt,
          status: 'spawned',
          targetAgent: event.target_agent,
        }]);
        break;

      case 'delegation_completed':
        deps.setSubagentEvents((prev) => upsertSubagentEvent(
          prev,
          event.id,
          { status: 'completed', summary: event.summary },
          {
            id: event.id,
            prompt: '',
            status: 'completed',
            summary: event.summary,
          },
        ));
        break;

      case 'delegation_failed':
        deps.setSubagentEvents((prev) => upsertSubagentEvent(
          prev,
          event.id,
          { status: 'failed', error: event.error },
          {
            id: event.id,
            prompt: '',
            status: 'failed',
            error: event.error,
          },
        ));
        break;

      case 'context_usage': {
        const usage = { used: event.used, total: event.total };
        deps.setContextUsage(usage);
        statusBarActions.setContextUsage(usage);
        break;
      }

      case 'precognition_result': {
        const noteNames = event.notes.map((note) => note.name);
        const noteList = noteNames.length > 0 ? noteNames.join(', ') : 'none';
        deps.addMessage({
          id: generateMessageId(),
          role: 'system' as Message['role'],
          content: `Auto-enriched with ${event.notes_count} notes: [${noteList}]`,
          timestamp: Date.now(),
          type: 'precognition',
        } as Message);
        break;
      }

      case 'mode_changed':
        deps.setChatMode(event.mode);
        statusBarActions.setChatMode(event.mode);
        break;

      case 'session_event':
        break;
    }
  };
}
