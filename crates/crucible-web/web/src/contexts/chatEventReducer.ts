import { statusBarActions } from '@/stores/statusBarStore';
import { generateMessageId, turnResponseId } from '@/lib/api';
import type {
  Message,
  ChatEvent,
  InteractionRequest,
  ToolCallDisplay,
  SubagentEvent,
  ContextUsage,
  ChatMode,
  TokenUsage,
} from '@/lib/types';

type ArraySetter<T> = (value: T[] | ((prev: T[]) => T[])) => void;

interface ChatEventReducerDeps {
  messages: () => Message[];
  currentStreamingMessageId: () => string | null;
  setCurrentStreamingMessageId: (id: string | null) => void;
  addMessage: (message: Message) => void;
  updateMessage: (id: string, updates: Partial<Message>) => void;
  appendToMessage: (id: string, content: string) => void;
  /** Insert a tool invocation as a transcript entry (role "tool"). */
  addToolMessage: (tool: ToolCallDisplay) => void;
  /** Update a tool transcript entry by call id. */
  updateToolMessage: (callId: string, updater: (tool: ToolCallDisplay) => ToolCallDisplay) => void;
  setSubagentEvents: ArraySetter<SubagentEvent>;
  setContextUsage: (usage: ContextUsage | null) => void;
  setChatMode: (mode: ChatMode) => void;
  onTitleChanged: (title: string) => void;
  setPendingInteraction: (request: InteractionRequest | null) => void;
  setError: (value: string | null) => void;
  setIsLoading: (value: boolean) => void;
  setIsStreaming: (value: boolean) => void;
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
  // A viewer that attaches mid-turn (page reload, PWA update, second pane)
  // has no streaming placeholder — sendMessage ran in another instance.
  // Materialize one instead of dropping the stream, so every viewer
  // converges on the same transcript.
  const ensureStreamingMessage = (): string => {
    const existing = deps.currentStreamingMessageId();
    if (existing) return existing;
    const id = generateMessageId();
    deps.addMessage({ id, role: 'assistant', content: '', timestamp: Date.now() });
    deps.setCurrentStreamingMessageId(id);
    deps.setIsStreaming(true);
    return id;
  };

  return (event: ChatEvent) => {
    switch (event.type) {
      case 'token': {
        deps.appendToMessage(ensureStreamingMessage(), event.content);
        break;
      }

      case 'tool_call':
      case 'tool_call_start': {
        // A tool call after streamed text is a segment boundary: close the
        // current text message so the narration between tools survives as
        // its own entry (message_complete only carries the FINAL response,
        // so folding everything into one message loses the in-between text).
        const streamingId = deps.currentStreamingMessageId();
        if (streamingId) {
          const current = deps.messages().find((m) => m.id === streamingId);
          if (current && current.content !== '') {
            deps.setCurrentStreamingMessageId(null);
          }
        }
        const toolName = 'title' in event ? event.title : ('name' in event ? event.name : '');
        const toolArgs = 'arguments' in event ? JSON.stringify(event.arguments ?? '') : '';
        deps.addToolMessage({
          id: event.id,
          name: toolName,
          args: toolArgs,
          status: 'running',
          callId: event.id,
        });
        break;
      }

      case 'tool_result':
        deps.updateToolMessage(event.id, (tool) => ({
          ...tool,
          result: event.result ?? '',
          status: 'complete',
          terminate: event.terminate ?? false,
        }));
        break;

      case 'tool_result_delta':
        deps.updateToolMessage(event.id, (tool) => ({
          ...tool,
          result: (tool.result ?? '') + event.delta,
        }));
        break;

      case 'tool_result_complete':
        deps.updateToolMessage(event.id, (tool) => ({ ...tool, status: 'complete' }));
        break;

      case 'tool_result_error':
        deps.updateToolMessage(event.id, (tool) => ({
          ...tool,
          result: event.error,
          status: 'error',
        }));
        break;

      case 'thinking': {
        const messageId = ensureStreamingMessage();
        const thinkingContent = deps.messages().find((message) => message.id === messageId)?.thinking?.content ?? '';
        deps.updateMessage(messageId, {
          thinking: {
            content: thinkingContent + event.content,
            isStreaming: true,
          },
        });
        break;
      }

      case 'message_complete': {
        const messageId = deps.currentStreamingMessageId();
        const thinkingData = messageId
          ? deps.messages().find((message) => message.id === messageId)?.thinking
          : undefined;
        const usage: TokenUsage | undefined = event.total_tokens ? {
          promptTokens: event.prompt_tokens ?? 0,
          completionTokens: event.completion_tokens ?? 0,
          totalTokens: event.total_tokens,
          cacheReadTokens: event.cache_read_tokens,
          cacheCreationTokens: event.cache_creation_tokens,
        } : undefined;
        // event.id is the TURN id (same one the user message carries), so
        // the assistant entry takes the derived response id — identical to
        // what sendMessage pre-created and what history reconstruction uses.
        // Segmented turns (text → tool → text) may have consumed the
        // response id on an earlier segment; later segments keep their own.
        const responseId = turnResponseId(event.id);
        const idTaken = deps.messages().some((m) => m.id === responseId && m.id !== messageId);
        if (messageId) {
          deps.updateMessage(messageId, {
            ...(idTaken ? {} : { id: responseId }),
            content: event.content,
            usage,
            ...(thinkingData ? {
              thinking: {
                content: thinkingData.content,
                isStreaming: false,
                tokenCount: thinkingData.content.length,
              },
            } : {}),
          });
        } else if (!deps.messages().some((m) => m.id === responseId)) {
          // Late attach with no streamed tokens seen: append the completed
          // message wholesale rather than dropping the turn's text.
          deps.addMessage({
            id: responseId,
            role: 'assistant',
            content: event.content,
            timestamp: Date.now(),
            usage,
          });
        }
        deps.setIsStreaming(false);
        deps.setIsLoading(false);
        deps.setCurrentStreamingMessageId(null);
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

      case 'connection': {
        // Transport reconnect — a transient banner ONLY. Must not touch the
        // streaming message, its content, or currentStreamingMessageId, or a
        // routine idle reconnect would corrupt/drop an in-flight turn.
        if (event.status === 'connected') {
          deps.setError(null);
        } else {
          deps.setError(event.message ?? 'Reconnecting…');
        }
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
        // Attach metadata to the most recent user message so PrecognitionBadge
        // can render on it. Daemon currently only fires precognition on the
        // first turn, so this is typically the first user message; finding
        // "most recent" keeps us correct if that ever changes.
        const lastUser = [...deps.messages()].reverse().find((m) => m.role === 'user');
        if (lastUser) {
          deps.updateMessage(lastUser.id, {
            precognition: {
              notesCount: event.notes_count,
              notes: event.notes,
            },
          });
        }
        break;
      }

      case 'mode_changed':
        deps.setChatMode(event.mode);
        statusBarActions.setChatMode(event.mode);
        break;

      case 'title_changed':
        deps.onTitleChanged(event.title);
        break;

      case 'session_event': {
        // The daemon echoes user_message over SSE with the turn's canonical
        // message_id — the same id sendMessage keyed its entry on, so dedup
        // is exact. Viewers that attached mid-turn get the prompt from here.
        if (event.event_type === 'user_message') {
          const data = event.data as { message_id?: string; content?: string } | null;
          if (data?.message_id && data.content !== undefined
            && !deps.messages().some((m) => m.id === data.message_id)) {
            deps.addMessage({
              id: data.message_id,
              role: 'user',
              content: data.content,
              timestamp: Date.now(),
            });
          }
        }
        break;
      }
    }
  };
}
