import { Component, createSignal, Show } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { nextChatMode } from './ChatModeControl';
import { useSessionScopeChips } from './SessionScopeChips';
import { SessionStatusChips } from './SessionStatusChips';
import { ComposerCard } from '@/components/composer/ComposerCard';
import type { ComposerChip } from '@/components/composer/ChipRow';
import { getBus } from '@/lib/bus';
import { useExecuteCommand } from '@/lib/query/commands';
import { statusBarStore } from '@/stores/statusBarStore';
import { sessionDefaultKiln } from '@/lib/session-scope';
import { kilnPathOf } from '@/stores/kilnStore';
import { ArrowUp, X } from '@/lib/icons';
import { ConnectionBanner } from '@/components/ui/ConnectionBanner';
import { InteractionHandler } from '@/components/interactions';

/**
 * The commit button's geometry, shared by send and cancel.
 *
 * They are the same control in two states, so they must not differ by a pixel
 * — a cancel that is wider than the send it replaces makes the whole trailing
 * cluster jump the moment a turn starts.
 */
const SEND_BASE =
  'focus-ring flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-colors';

export const ChatInput: Component = () => {
  const { sessionId, sendMessage, isLoading, isStreaming, cancelStream, error, connectionStatus, retryConnection, chatMode, availableModes, switchMode, addSystemMessage, clearMessages, pendingInteraction, respondToInteraction } = useChatSafe();
  const { currentSession, cancelCurrentOperation, availableModels, switchModel } = useSessionSafe();
  const [input, setInput] = createSignal('');
  let formRef: HTMLFormElement | undefined;

  const session = () => currentSession();
  // Bound to the accessor, not to an id: the composer outlives the session on
  // screen, so a command typed after a tab switch must reach the session the
  // user is looking at.
  const runCommand = useExecuteCommand(() => session()?.session_id ?? '');
  // Sending is allowed whenever a session is selected and no turn is in flight.
  // Lifecycle state (paused/ended) is NOT a gate: the daemon transparently
  // revives an idle session on send, so an ended session is never a dead end.
  const canSend = () => {
    const s = session();
    return !!s && !isLoading() && input().trim().length > 0;
  };

  // Palette "Switch Model" opens the same picker as the chip below.
  // Gate on the focused chat so split panes don't all pop their pickers
  // (activeSessionId tracks tab/pane focus via the window store).
  getBus().on('switchModel', () => {
    const active = statusBarStore.activeSessionId();
    if (active && sessionId() !== active) return;
    if (session()) {
      (formRef?.querySelector('[data-testid="model-picker-button"]') as HTMLElement | null)?.click();
    }
  });

  const handleSubmit = async (e?: Event) => {
    e?.preventDefault();
    const message = input().trim();
    if (!message || !canSend()) return;

    setInput('');

    // Slash command detection: route to command endpoint
    if (message.startsWith('/')) {
      const s = session();
      if (!s) return;

      try {
        const result = await runCommand.mutateAsync(message);
        // Special handling for /clear
        if (message.startsWith('/clear')) {
          clearMessages();
        }
        addSystemMessage(result.result);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Command failed';
        addSystemMessage(`Error: ${errorMsg}`);
      }
      return;
    }

    await sendMessage(message);
  };

  // Shift+Tab cycles chat mode (Ask → Plan → Auto). Enter-to-send is the
  // ComposerCard's default, applied after this returns without claiming the key.
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Tab' && e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      // `nextChatMode` returns the current mode unchanged when it cannot
      // advance (the daemon no longer offers it). POSTing that re-sends a mode
      // `set_mode` rejects and toasts an error on every keypress.
      const next = nextChatMode(chatMode(), availableModes().map((m) => m.id));
      if (next !== chatMode()) switchMode(next);
    }
  };

  const handleCancel = async () => {
    cancelStream();
    await cancelCurrentOperation();
  };



  // Show the model id as-is. Prefixing with the provider's wire *type* turned
  // every model into "openai/…" for any OpenAI-compatible endpoint (e.g. a
  // local GLM server) — misleading, and redundant since the picker is already
  // scoped to the session's provider. Ids that carry their own namespace
  // (OpenRouter's "openai/gpt-4o") keep their natural form either way.
  const formatModelDisplay = (model: string) => model;

  const currentModel = () => {
    const s = currentSession();
    if (!s?.agent_model) return 'Select model';
    return formatModelDisplay(s.agent_model);
  };

  const handleModelSelect = (model: string) => {
    void switchModel(model);
  };

  const scopeChips = useSessionScopeChips();
  // Created ONCE. The list below is rebuilt on every signal it reads, and a
  // component created inside it would remount (and refetch) each time.
  const statusChips = <SessionStatusChips />;

  /**
   * The live session's chip row, as data for the shared `ChipRow`: the
   * model, the mode, then the session's scope (project, kiln — attach and
   * detach mid-session, Crucible Shell design 4a/5a) and the plugin status
   * chips (review policy and the like). The same row the draft draws, with
   * fewer and simpler chips.
   *
   * Each entry states a `priority`, which is both the draw order and the
   * order the row folds them in when the pane is too narrow to hold them.
   */
  const liveChips = (): ComposerChip[] => [
    {
      key: 'model',
      // The row folds from the right when the pane narrows, so the two
      // controls a user reaches for mid-turn go first. The scope chips state
      // 30 and 40 in `useSessionScopeChips`; the status chips report rather
      // than set, so they are the first to fold.
      priority: 10,
      label: 'Model',
      value: currentSession()?.agent_model ?? '',
      options: availableModels().map((m) => ({ value: m, label: formatModelDisplay(m) })),
      onSelect: handleModelSelect,
      disabled: !session() || isLoading(),
      testid: 'model-picker-button',
      select: { placeholder: currentModel(), optionTestidPrefix: 'model-option' },
    },
    { key: 'mode', priority: 20, label: 'Mode', value: chatMode(), render: 'mode' },
    ...scopeChips(),
    { key: 'status', priority: 90, label: 'Status', value: '', render: 'custom', element: statusChips },
  ];

  return (
    <form
      ref={formRef}
      onSubmit={handleSubmit}
      // NO `border-t`. A rule here boxed the composer in and cut the
      // conversation off at a hard line; the transcript now fades into this
      // strip instead (see `.transcript-fade`), which carries the same
      // "there is more above" meaning without drawing an edge.
      //
      // `px-4` OUTSIDE the measure, exactly as MessageList has it. Putting
      // the padding inside instead made the composer 32px narrower than the
      // transcript above it, so the two column edges did not line up.
      class="px-4 pb-3 pt-1"
      data-testid="chat-input-form"
    >
      <div class="mx-auto w-full max-w-[var(--chat-measure)]">
      {/* A dropped stream and a daemon-side failure are different faults, so
          they get different affordances. The stream is skippable-waitable, so
          it gets the same banner (and the same retry) the terminal has; the
          daemon error has nothing to re-issue from here and stays a statement. */}
      <Show when={connectionStatus() === 'reconnecting'}>
        <ConnectionBanner
          class="mb-2"
          tone="transient"
          message={error() ?? 'Reconnecting…'}
          retryLabel="Retry now"
          onRetry={retryConnection}
          testid="chat-connection-banner"
          retryTestid="chat-connection-retry"
        />
      </Show>

      <Show when={connectionStatus() !== 'reconnecting' && error()}>
        <div class="mb-2 px-2 py-1 text-sm text-error bg-error-dark/20 rounded">
          {error()}
        </div>
      </Show>

      {/* No "no active session" notice here — MessageList already renders
          the full empty state above; repeating it in the input strip read
          as two stacked prompts. */}

      {/* The gate, docked ON the prompt.
          A permission used to be drawn where the agent hit it, in the middle
          of the transcript — which scrolls. The one control the session is
          parked on could therefore be off-screen, and the composer below it
          looked ready to take a message it would not send. The card now sits
          against the prompt, which never scrolls away, and the transcript
          keeps a one-line record at the point of the request instead. */}
      <Show when={pendingInteraction()}>
        {(request) => (
          <div class="composer-dock" data-testid="composer-dock">
            <InteractionHandler request={request()} onRespond={respondToInteraction} />
          </div>
        )}
      </Show>

      <ComposerCard
        docked={!!pendingInteraction()}
        value={input}
        setValue={setInput}
        // `[[note]]` completion needs the DIRECTORY; the session carries a
        // registry name. Unresolved (kiln-less, or the registry not back yet)
        // completes against nothing rather than against the data root.
        kilnPath={() => {
          const s = currentSession();
          return (s ? kilnPathOf(sessionDefaultKiln(s)) : null) ?? undefined;
        }}
        placeholder={session() ? 'Type a message...' : 'Select a session first...'}
        disabled={!session() || isLoading()}
        testid="chat-input"
        onSubmit={() => void handleSubmit()}
        onKeyDown={handleKeyDown}
        chips={liveChips()}
        action={
          <Show
            when={isStreaming()}
            fallback={
              <button
                type="submit"
                disabled={!canSend()}
                aria-label="Send message"
                title="Send (Enter)"
                classList={{
                  // A DISABLED send still has to be visible. It was
                  // `bg-transparent`, which read as "there is no send button
                  // here" rather than "you have not typed anything" — the
                  // control vanished exactly when a new user needed to find
                  // it. It keeps its fill and loses its colour instead.
                  [SEND_BASE]: true,
                  'bg-primary text-on-primary hover:bg-primary-hover': !!canSend(),
                  'bg-control text-muted-dark cursor-not-allowed': !canSend(),
                }}
                data-testid="send-button"
              >
                <ArrowUp class="w-4 h-4" />
              </button>
            }
          >
            <button
              type="button"
              onClick={handleCancel}
              aria-label="Cancel response"
              title="Stop the response"
              class={`${SEND_BASE} bg-error text-white hover:bg-error-dark`}
              data-testid="cancel-button"
            >
              <X class="w-4 h-4" />
            </button>
          </Show>
        }
      />

      </div>
    </form>
  );
};
