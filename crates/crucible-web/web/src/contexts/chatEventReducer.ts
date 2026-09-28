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
    switch (event.type) {
      case 'token':
      case 'thinking':
      case 'tool_call':
      case 'tool_result':
      case 'tool_result_delta':
      case 'tool_result_complete':
      case 'tool_result_error':
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
        if ((event.status === 'failed' || event.status === 'handler_cancelled') && event.error) {
          deps.setError(`${event.error} (turn_${event.status})`);
        }
        closeTurn();
        break;
      }

      case 'error':
        deps.setError(`${event.message} (${event.code})`);
        deps.addErrorNotice(`Error: ${event.message}`);
        closeTurn();
        break;

      case 'connection': {
        // Transport reconnect — a transient banner ONLY.
        deps.setConnectionStatus(event.status === 'connected' ? 'connected' : 'reconnecting');
        deps.setError(event.status === 'connected' ? null : (event.message ?? 'Reconnecting…'));
        break;
      }

      case 'interaction_requested': {
        const { type: _eventType, ...requestData } = event;
        deps.setPendingInteraction(requestData as unknown as InteractionRequest);
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

      // The transcript items carry these: the delegation rows, the notes of
      // Precognition, and every `session_event` that changes the transcript
      // (`user_message`, `tool_call_update`, `context_cleared`). The store
      // refetches the snapshot on a `stream_gap`.
      case 'delegation_spawned':
      case 'delegation_completed':
      case 'delegation_failed':
      case 'precognition_result':
      case 'session_event':
        break;

      // The catalog is a query of its own; `lib/query/routes/session.ts`
      // invalidates it. The pane shows nothing for the event.
      case 'commands_changed':
        break;

      // `transcriptStore` applies the ops before the reducer runs.
      case 'transcript':
        break;
    }
  };
}
