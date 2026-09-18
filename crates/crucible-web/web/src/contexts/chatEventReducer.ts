import { statusBarActions } from '@/stores/statusBarStore';
import {
  generateMessageId,
  turnResponseId,
  turnSegmentId,
  turnThinkingId,
  stripFrozenPrefix,
  estimateThinkingTokens,
} from '@/lib/turn';
import type {
  Message,
  ChatEvent,
  InteractionRequest,
  ToolCallDisplay,
  SubagentEvent,
  ChatMode,
  TokenUsage,
  ConnectionStatus,
} from '@/lib/types';

type ArraySetter<T> = (value: T[] | ((prev: T[]) => T[])) => void;

interface ChatEventReducerDeps {
  messages: () => Message[];
  currentStreamingMessageId: () => string | null;
  setCurrentStreamingMessageId: (id: string | null) => void;
  addMessage: (message: Message) => void;
  /** Insert a message at a known transcript position (mid-turn echoes that
   * must land after the streaming block, not inside it). */
  insertMessageAfter: (index: number, message: Message) => void;
  updateMessage: (id: string, updates: Partial<Message>) => void;
  appendToMessage: (id: string, content: string) => void;
  /** Insert a tool invocation as a transcript entry (role "tool"). */
  addToolMessage: (tool: ToolCallDisplay) => void;
  /** Update a tool transcript entry by call id. */
  updateToolMessage: (callId: string, updater: (tool: ToolCallDisplay) => ToolCallDisplay) => void;
  setSubagentEvents: ArraySetter<SubagentEvent>;
  setChatMode: (mode: ChatMode) => void;
  /** Called when the daemon names a mode absent from our list — it was
   * declared after the mount-time fetch, so the list needs refreshing. */
  onUnknownMode?: (mode: ChatMode) => void;
  onTitleChanged: (title: string) => void;
  setPendingInteraction: (request: InteractionRequest | null) => void;
  setError: (value: string | null) => void;
  /** Transport health of the SSE stream, separate from `setError`. The error
   * line is a message; this is the STATE that decides whether the surface owes
   * the user a retry control. */
  setConnectionStatus: (value: ConnectionStatus) => void;
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

/**
 * The state a tool is left in when its turn ended without an answer for it:
 * a partial result completes it, no result at all surfaces the error. The
 * live reducer sweeps with this at turn end, and the history fold reuses it
 * so a reload derives the same state the live transcript showed — not a
 * blanket 'complete' the events never said.
 */
export function finalizeDanglingTool(tool: ToolCallDisplay): ToolCallDisplay {
  const hasResult = tool.result != null && tool.result !== '';
  return {
    ...tool,
    status: hasResult ? 'complete' : 'error',
    result: hasResult ? tool.result : 'tool did not complete',
  };
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
    deps.addMessage({ id, role: 'assistant', content: '',
          placeholder: true, timestamp: Date.now() });
    deps.setCurrentStreamingMessageId(id);
    deps.setIsStreaming(true);
    return id;
  };

  // Text that streamed before each tool call is frozen into its own assistant
  // bubble at the segment boundary (see the tool_call case). The daemon's
  // message_complete carries the WHOLE turn's accumulated text, so we must
  // strip these already-rendered prefixes off the final bubble or the
  // narration renders twice. Reset per turn.
  let frozenSegments: string[] = [];

  // A turn can complete (or error) with a tool still marked "running" — no
  // tool_result ever arrived. Left alone, ToolCard shows a perpetual spinner.
  // Finalize each: keep any partial result as a completed card, else surface
  // it as an error so the transcript isn't silently misleading.
  const finalizeDanglingTools = () => {
    const runningCallIds = deps
      .messages()
      .filter((m) => m.role === 'tool' && m.toolCall?.status === 'running')
      .map((m) => m.toolCall!.callId)
      .filter((id): id is string => typeof id === 'string' && id.length > 0);
    for (const callId of runningCallIds) {
      deps.updateToolMessage(callId, finalizeDanglingTool);
    }
  };

  // Thinking that streamed onto a segment frozen at a tool boundary never
  // receives the messageId-targeted finalization in message_complete (the
  // streaming id was cleared at the boundary), so sweep every message still
  // marked thinking-in-progress when the turn ends — no bubble may be left
  // saying "Thinking…" forever.
  const finalizeStreamingThinking = () => {
    for (const m of deps.messages()) {
      if (m.thinking?.isStreaming) {
        deps.updateMessage(m.id, {
          thinking: {
            content: m.thinking.content,
            isStreaming: false,
            tokenCount: estimateThinkingTokens(m.thinking.content),
          },
        });
      }
    }
  };

  // One turn, one closing rule: every way a turn ends — completion, error, a
  // cancel from THIS client or a foreign one (the daemon records `ended` and
  // every subscriber receives it) — sweeps the same state. No dangling tool
  // left "running", no bubble left "Thinking…", no stale streaming id.
  const closeTurn = () => {
    finalizeDanglingTools();
    finalizeStreamingThinking();
    frozenSegments = [];
    deps.setIsStreaming(false);
    deps.setIsLoading(false);
    deps.setCurrentStreamingMessageId(null);
  };

  return (event: ChatEvent) => {
    switch (event.type) {
      case 'token': {
        deps.appendToMessage(ensureStreamingMessage(), event.content);
        break;
      }

      case 'tool_call': {
        // A tool call after streamed text is a segment boundary: close the
        // current text message so the narration between tools survives as
        // its own entry (message_complete only carries the FINAL response,
        // so folding everything into one message loses the in-between text).
        const streamingId = deps.currentStreamingMessageId();
        if (streamingId) {
          const current = deps.messages().find((m) => m.id === streamingId);
          if (current && current.content !== '') {
            // Record what we're freezing so message_complete can strip it off
            // the turn's accumulated text and not re-render this narration.
            frozenSegments.push(current.content);
            deps.setCurrentStreamingMessageId(null);
          } else if (current?.thinking && current.thinking.content !== '') {
            // Thinking-only segment: the model reasoned and went straight to a
            // tool without narrating. Nothing to freeze (no text for
            // stripFrozenPrefix to strip, and pushing '' would misalign the
            // index text_segment matches on), but the segment must still
            // CLOSE. Left open, every later thinking delta appended to this
            // same block and the tool cards collapsed into one run beside it,
            // losing which reasoning led to which call.
            deps.updateMessage(streamingId, {
              thinking: {
                content: current.thinking.content,
                isStreaming: false,
                tokenCount: estimateThinkingTokens(current.thinking.content),
              },
            });
            deps.setCurrentStreamingMessageId(null);
          }
        }
        // `title` is the only tool-name field on the wire. The `name` fallback
        // here existed for `tool_call_start`, an SSE name the server has never
        // been able to send.
        const toolName = event.title;
        const toolArgs = 'arguments' in event ? JSON.stringify(event.arguments ?? '') : '';
        deps.addToolMessage({
          id: event.id,
          name: toolName,
          args: toolArgs,
          status: 'running',
          callId: event.id,
          // Computed daemon-side. Absent on recordings that predate the
          // field — the card then shows no summary line, and the expanded
          // args still render in full.
          display: 'display' in event ? (event.display as ToolCallDisplay['display']) : undefined,
          // Decided before this event was emitted, so the marker renders with
          // the card instead of appearing a beat later.
          autoApproved: 'auto_approved' in event ? (event.auto_approved as string) : undefined,
          // The call's proposed edits, decided by the daemon — not re-derived
          // here from the tool name.
          diffs: 'diffs' in event ? (event.diffs as ToolCallDisplay['diffs']) : undefined,
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

      case 'segment_complete': {
        // The daemon marks a text → tool boundary explicitly (it fires just
        // before the tool_call). Freezing here — instead of relying on the
        // tool_call fallback below — lets us give the segment a CANONICAL id
        // (${message_id}-seg-${index}) so live viewers and reloaded viewers
        // converge on the same bubble. The whole turn's text still arrives in
        // message_complete; frozenSegments records what's already rendered so
        // that final bubble strips this prefix.
        const canonicalId = turnSegmentId(event.message_id, event.index);
        // Replay / duplicate signal — the segment bubble already exists.
        if (deps.messages().some((m) => m.id === canonicalId)) {
          break;
        }
        const streamingId = deps.currentStreamingMessageId();
        if (streamingId) {
          const streaming = deps.messages().find((m) => m.id === streamingId);
          if (streaming) {
            // Happy path: freeze the open streaming text into its canonical
            // segment bubble (rename + close). The tool_call that follows sees
            // no streaming message, so its fallback freeze does NOT run — this
            // segment is recorded in frozenSegments exactly once.
            frozenSegments.push(streaming.content);
            deps.updateMessage(streamingId, { id: canonicalId });
            deps.setCurrentStreamingMessageId(null);
            break;
          }
        }
        // No open streaming message. If the tool_call fallback already froze
        // this text (unusual ordering), it lives under a random id and
        // frozenSegments already accounts for it — adopt the canonical id on
        // that bubble rather than duplicating.
        if (frozenSegments[event.index] === event.content) {
          const stale = deps
            .messages()
            .find((m) => m.role === 'assistant' && m.content === event.content && m.id !== canonicalId);
          if (stale) deps.updateMessage(stale.id, { id: canonicalId });
          break;
        }
        // Late attach: never saw the streamed tokens. Materialize the segment
        // bubble under its canonical id and record it as frozen so a later
        // message_complete strips it.
        frozenSegments.push(event.content);
        deps.addMessage({
          id: canonicalId,
          role: 'assistant',
          content: event.content,
          timestamp: Date.now(),
        });
        break;
      }

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
          cacheReadTokens: event.cache_read_tokens ?? undefined,
          cacheCreationTokens: event.cache_creation_tokens ?? undefined,
        } : undefined;
        // event.id is the TURN id (same one the user message carries), so
        // the assistant entry takes the derived response id — identical to
        // what sendMessage pre-created and what history reconstruction uses.
        // Segmented turns (text → tool → text) may have consumed the
        // response id on an earlier segment; later segments keep their own.
        const responseId = turnResponseId(event.id);
        // Who else holds the id the answer must carry? Only two bubbles can:
        // a spent placeholder of THIS turn, or a bubble history already
        // reconstructed for it.
        const holder = deps.messages().find((m) => m.id === responseId && m.id !== messageId);
        // A spent placeholder carries no text — the turn reasoned, went
        // straight to a tool, and the reducer closed that bubble, while
        // dispatchTurn had already renamed it to the canonical id. It is not
        // the answer, so it gives the id up: history reconstruction derives
        // the SAME id for the answer and the merge dedupes by id alone, so an
        // answer left under a client-minted id renders a second time the
        // moment history loads over it.
        // Provenance, not emptiness: a history bubble that answered with no
        // text and only tool calls has the same shape as a placeholder.
        const spentPlaceholder = holder?.role === 'assistant' && holder.placeholder === true && holder.content === '';
        if (holder && spentPlaceholder) {
          deps.updateMessage(holder.id, { id: turnThinkingId(event.id) });
        }
        // A holder that DOES carry text is the turn's answer already (history,
        // or a replayed completion). Leave it alone rather than collide two
        // messages onto one id.
        const idTaken = holder !== undefined && !spentPlaceholder;
        // event.content is the ENTIRE turn's accumulated text. Segments frozen
        // before earlier tool calls already render as their own bubbles, so the
        // final bubble must carry only the trailing text. stripFrozenPrefix is
        // shared with history reconstruction so both converge on the same
        // final-bubble content.
        const frozenPrefix = frozenSegments.join('');
        const finalContent = stripFrozenPrefix(event.content, frozenSegments);
        if (messageId) {
          deps.updateMessage(messageId, {
            ...(idTaken ? {} : { id: responseId }),
            content: finalContent,
            usage,
            completedAt: Date.now(),
            ...(thinkingData ? {
              thinking: {
                content: thinkingData.content,
                isStreaming: false,
                tokenCount: estimateThinkingTokens(thinkingData.content),
              },
            } : {}),
          });
        } else if (!deps.messages().some((m) => m.id === responseId)
          && (finalContent !== '' || frozenPrefix === '')) {
          // Late attach with no active streaming message: append the trailing
          // segment as its own bubble rather than dropping the turn's text.
          // Skip when the frozen segments already cover the whole turn (no
          // trailing text) so we don't add an empty bubble.
          deps.addMessage({
            id: responseId,
            role: 'assistant',
            content: finalContent,
            timestamp: Date.now(),
            usage,
            completedAt: Date.now(),
          });
        } else if (usage && frozenPrefix !== '') {
          // Segments covered the whole turn, so there is no trailing bubble to
          // carry the token usage — put it on the turn's last frozen segment
          // so totals still render.
          const lastAssistant = [...deps.messages()]
            .reverse()
            .find((m) => m.role === 'assistant');
          if (lastAssistant) deps.updateMessage(lastAssistant.id, { usage });
        }
        finalizeDanglingTools();
        finalizeStreamingThinking();
        frozenSegments = [];
        // A reply the provider cut off gets a note of its own, under the
        // bubble rather than inside it: the text is the model's, the note is
        // the daemon's. The daemon also WORDS it — `stop_notice` carries the
        // string, so this page holds no second copy to drift from.
        const stopNotice = event.stop_notice;
        if (stopNotice) {
          deps.addMessage({
            id: `${event.id}-stop-reason`,
            role: 'system',
            content: stopNotice,
            timestamp: Date.now(),
          });
        }
        closeTurn();
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
        closeTurn();
        break;
      }

      case 'connection': {
        // Transport reconnect — a transient banner ONLY. Must not touch the
        // streaming message, its content, or currentStreamingMessageId, or a
        // routine idle reconnect would corrupt/drop an in-flight turn.
        deps.setConnectionStatus(event.status === 'connected' ? 'connected' : 'reconnecting');
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
          targetAgent: event.target_agent ?? undefined,
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
              // `notes` is `#[serde(default)]` in Rust, so an older recording
              // carries the count and no list.
              notes: (event.notes ?? []).map((note) => ({
                name: note.name,
                relevance: note.relevance ?? 0,
              })),
            },
          });
        }
        break;
      }

      case 'mode_changed':
        deps.setChatMode(event.mode);
        statusBarActions.setChatMode(event.mode);
        deps.onUnknownMode?.(event.mode);
        break;

      case 'title_changed':
        deps.onTitleChanged(event.title);
        break;

      case 'session_event': {
        // The turn ENDED: the daemon records `ended` with its reason and
        // broadcasts it to every subscriber — the canceller's client, other
        // panes on the session, and future replays alike. Error-prefixed
        // reasons arrive as typed `error` events instead, so anything here
        // simply means "the turn is over": sweep the same state a completion
        // would. This is what makes a cancel issued from ANOTHER client stop
        // this pane's spinner — the old client-side patch could only ever
        // close the turn that THIS client cancelled.
        if (event.event === 'ended') {
          closeTurn();
          break;
        }

        // Late file-diff content for a call already announced by a prior
        // `tool_call` (an ACP agent that announces without `rawInput`).
        // Same merge rule as the TUI: the update carries the call's full
        // diff set, so it replaces — and an empty/missing set is a no-op,
        // not a wipe.
        if (event.event === 'tool_call_diff_update') {
          const data = event.data as { call_id?: unknown; diffs?: ToolCallDisplay['diffs'] } | null;
          const callId = typeof data?.call_id === 'string' ? data.call_id : undefined;
          if (callId && data && Array.isArray(data.diffs) && data.diffs.length > 0) {
            deps.updateToolMessage(callId, (tool) => ({ ...tool, diffs: data.diffs }));
          }
          break;
        }

        // The daemon's event forwarder writes this straight to our connection
        // when its broadcast cursor falls off the ring: N events are gone and
        // nothing later mentions them. It arrives as a passthrough because it is
        // minted per connection, not by a session, so it has no typed payload.
        //
        // Surfaced rather than logged: a transcript with an invisible hole is
        // permanently and silently wrong, and reloading the session is the only
        // way back — so the user has to be told, and told what to do.
        if (event.event === 'stream_gap') {
          const dropped = (event.data as { dropped?: number } | null)?.dropped;
          deps.setError(
            dropped === undefined
              ? 'Event stream fell behind and events were dropped — this conversation is incomplete. Reload to see it whole.'
              : `Event stream fell behind and ${dropped} events were dropped — this conversation is incomplete. Reload to see it whole.`,
          );
          // The turn may have ENDED inside the lost span, so no later
          // message_complete is guaranteed to sweep a bubble left thinking.
          // This event never re-fires, so nothing after it would.
          finalizeStreamingThinking();
          break;
        }

        // The daemon echoes user_message over SSE with the turn's canonical
        // message_id — the same id sendMessage keyed its entry on, so dedup
        // is exact. Viewers that attached mid-turn get the prompt from here.
        if (event.event === 'user_message') {
          // A new user turn begins — drop any frozen-segment state that a
          // prior turn left behind (e.g. one that errored without a clean
          // message_complete) so it can't strip the next turn's final bubble.
          frozenSegments = [];
          const data = event.data as { message_id?: string; content?: string } | null;
          if (data?.message_id && data.content !== undefined
            && !deps.messages().some((m) => m.id === data.message_id)) {
            // Adopt an optimistic temp entry with the same content instead of
            // duplicating it — the echo can arrive before the send POST (or a
            // sibling provider's dispatch) canonicalized the temp id. Temp ids
            // are client-minted `msg_…` (underscore); daemon ids are `msg-…`.
            const temp = deps
              .messages()
              .find((m) => m.role === 'user' && m.content === data.content && /^msg_/.test(m.id));
            if (temp) {
              deps.updateMessage(temp.id, { id: data.message_id });
            } else {
              // The daemon admits one turn at a time, so an echo landing
              // while a turn still streams belongs to a turn QUEUED behind
              // it (the cancel window). It goes at the end of the streaming
              // BLOCK — directly after the streaming message — never inside
              // the tool run that may stream below it.
              const streamingId = deps.currentStreamingMessageId();
              const streamIdx = streamingId
                ? deps.messages().findIndex((m) => m.id === streamingId)
                : -1;
              const entry: Message = {
                id: data.message_id,
                role: 'user',
                content: data.content,
                timestamp: Date.now(),
              };
              if (streamIdx !== -1) {
                deps.insertMessageAfter(streamIdx, entry);
              } else {
                deps.addMessage(entry);
              }
            }
          }
        }
        break;
      }
    }
  };
}
