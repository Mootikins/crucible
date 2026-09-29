import { statusBarActions } from '@/stores/statusBarStore';
import type { ChatEvent, InteractionRequest, ChatMode, ConnectionStatus } from '@/lib/types';

/**
 * The session state around the transcript: streaming and loading, errors,
 * the open interaction, the mode and the title.
 *
 * The transcript itself is not here. The daemon folds the events, and
 * `transcriptStore` applies the ops of the `transcript` frames. The reducer
 * reads the other events only.
 */
interface ChatEventReducerDeps {
  setChatMode: (mode: ChatMode) => void;
  /** Called when the daemon names a mode absent from our list — it was
   * declared after the mount-time fetch, so the list needs refreshing. */
  onUnknownMode?: (mode: ChatMode) => void;
  onTitleChanged: (title: string) => void;
  setPendingInteraction: (request: InteractionRequest | null) => void;
  setError: (value: string | null) => void;
  /** Keeps a daemon error in the transcript as a local notice. The error
   * line alone can give way to a reconnect banner. */
  addErrorNotice: (message: string) => void;
  /** Transport health of the SSE stream, separate from `setError`. The error
   * line is a message; this is the STATE that decides whether the surface owes
   * the user a retry control. */
  setConnectionStatus: (value: ConnectionStatus) => void;
  setIsLoading: (value: boolean) => void;
  isStreaming: () => boolean;
  setIsStreaming: (value: boolean) => void;
}

export function createChatEventReducer(deps: ChatEventReducerDeps) {
  // A turn that another client started streams here too. Its first
  // activity marks the session busy, so a send from this pane queues.
  const markStreaming = () => {
    if (!deps.isStreaming()) deps.setIsStreaming(true);
  };

  // Every way a turn ends reaches every subscriber as `turn_finished` (or
  // `message_complete` before it), and this clears the busy state.
  const closeTurn = () => {
    deps.setIsStreaming(false);
    deps.setIsLoading(false);
  };

  return (event: ChatEvent) => {
    // `ChatEvent` mixes two tags: `type` (the client's own `connection`
    // frame, and `TranscriptFrame`) and `event` (every `SessionEventPayload`
    // variant — see `lib/types.ts`). `'type' in event` is false for every
    // `SessionEventPayload` member, so it narrows the union correctly.
    if ('type' in event) {
      if (event.type === 'connection') {
        // Transport reconnect — a transient banner ONLY.
        deps.setConnectionStatus(event.status === 'connected' ? 'connected' : 'reconnecting');
        deps.setError(event.status === 'connected' ? null : (event.message ?? 'Reconnecting…'));
        return;
      }
      // event.type === 'transcript': transcriptStore applies the ops
      // before the reducer runs.
      return;
    }

    switch (event.event) {
      case 'text_delta':
      case 'thinking':
      case 'tool_call':
      case 'tool_result':
      case 'segment_complete':
        markStreaming();
        break;

      case 'message_complete':
        closeTurn();
        break;

      case 'turn_finished': {
        // A failed turn and a turn that a handler cancelled (for example the
        // loop guard) carry the reason in `error`. The transcript shows the
        // notice of a failed turn; the error line says it too.
        const { status, error } = event.data;
        if ((status === 'failed' || status === 'handler_cancelled') && error) {
          deps.setError(`${error} (turn_${status})`);
        }
        closeTurn();
        break;
      }

      case 'interaction_requested': {
        const { request_id, request } = event.data;
        deps.setPendingInteraction({ ...request, id: request_id } as InteractionRequest);
        break;
      }

      case 'mode_changed': {
        // `#[serde(default)]` on the Rust field makes the schema mark it
        // optional for a READER — an old recording could omit it — though
        // every live event carries it. The empty-string fallback matches
        // that Rust default.
        const mode = event.data.mode ?? '';
        deps.setChatMode(mode);
        statusBarActions.setChatMode(mode);
        deps.onUnknownMode?.(mode);
        break;
      }

      case 'title_changed':
        deps.onTitleChanged(event.data.title ?? '');
        break;

      // The transcript items carry these: user turns, tool-call updates,
      // context markers, delegation rows and the notes of Precognition —
      // `transcriptStore` applies them from the co-emitted `transcript`
      // frame, so this reducer does nothing with the named event itself.
      case 'user_message':
      case 'context_cleared':
      case 'context_injected':
      case 'tool_call_update':
      case 'delegation_spawned':
      case 'delegation_completed':
      case 'delegation_failed':
      case 'precognition_complete':
      case 'interaction_completed':
      case 'post_llm_call':
        break;

      // The catalog is a query of its own; `lib/query/routes/session.ts`
      // invalidates it. The pane shows nothing for the event.
      case 'commands_changed':
        break;

      // Session-settings acknowledgements other than mode/title: each has
      // its own reader (the session config panels, the status bar), not
      // this reducer.
      case 'model_switched':
      case 'scope_changed':
      case 'system_prompt_changed':
      case 'precognition_toggled':
      case 'context_strategy_changed':
      case 'plugin_approval_changed':
      case 'plugin_turn_limit_changed':
        break;

      // Setup-phase notices: `lib/query/routes/session.ts` and the status
      // panels read these directly off the stream; the chat pane does not.
      case 'session_initialized':
      case 'providers_listed':
      case 'context_limit_resolved':
      case 'workspace_indexed':
      case 'kiln_notes_indexed':
      case 'plugins_discovered':
      case 'mcp_servers_ready':
      case 'acp_resume_fallback':
        break;

      // Delegated job lifecycle beyond the three delegation_* events above:
      // no per-session chat rendering.
      case 'bash_job_spawned':
      case 'bash_job_completed':
      case 'bash_job_failed':
      case 'background_job_completed':
        break;

      // Review/undo: the diff panel's own store reads these.
      case 'review_changed':
      case 'session_undo':
        break;

      // Session-wide notifications: the toast store reads these.
      case 'notification_added':
      case 'notification_dismissed':
        break;

      // Workflow-engine progress: the workflow panel's own store.
      case 'workflow.step_started':
      case 'workflow.step_completed':
      case 'workflow.gate_reached':
      case 'workflow.gate_approved':
      case 'workflow.completed':
      case 'workflow.assessed':
      case 'workflow.failed':
      case 'workflow.cancelled':
        break;

      // Daemon-wide system events: none are addressed to one session's chat
      // pane. File watch, kiln indexing, UI config, webhooks, replay,
      // plugin surfaces/publications, proposals, session lifecycle and the
      // transport's own gap marker each have their own reader elsewhere
      // (the fs store, the surfaces store, the proposals store, the session
      // list, `EventSource`'s dedicated `stream_gap` listener).
      case 'file_changed':
      case 'file_deleted':
      case 'file_moved':
      case 'classification_required':
      case 'process_complete':
      case 'ui_style_changed':
      case 'status_items_changed':
      case 'stream_gap':
      case 'note:created':
      case 'note:modified':
      case 'note:deleted':
      case 'note:renamed':
      case 'base:changed':
      case 'webhook:received':
      case 'replay_complete':
      case 'session:created':
      case 'session:ended':
      case 'surface_changed':
      case 'publication_changed':
      case 'proposal_changed':
        break;

      /* v8 ignore next 6 -- unreachable by construction: every real
       * `SessionEventPayload` variant has a case above, and a new one fails
       * `bun run typecheck` here at compile time (see the type budget note
       * in `lib/types.ts`) before it could ever reach this branch at run
       * time. */
      default: {
        const unhandled: never = event;
        void unhandled;
      }
    }
  };
}
