import { Component, createSignal, Show, onCleanup } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { ChatModeControl, nextChatMode } from './ChatModeControl';
import { SessionScopeChips } from './SessionScopeChips';
import { ChipSelect } from '@/components/composer/ChipSelect';
import { ComposerCard } from '@/components/composer/ComposerCard';
import { executeCommand } from '@/lib/api';
import { statusBarStore } from '@/stores/statusBarStore';
import { ArrowUp, X } from '@/lib/icons';
export const ChatInput: Component = () => {
  const { sessionId, sendMessage, isLoading, isStreaming, cancelStream, error, chatMode, availableModes, switchMode, addSystemMessage, clearMessages } = useChatSafe();
  const { currentSession, cancelCurrentOperation, availableModels, switchModel } = useSessionSafe();
  const [input, setInput] = createSignal('');
  let formRef: HTMLFormElement | undefined;

  const session = () => currentSession();
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
  const onSwitchModelEvent = () => {
    const active = statusBarStore.activeSessionId();
    if (active && sessionId() !== active) return;
    if (session()) {
      (formRef?.querySelector('[data-testid="model-picker-button"]') as HTMLElement | null)?.click();
    }
  };
  window.addEventListener('crucible:switch-model', onSwitchModelEvent);
  onCleanup(() => window.removeEventListener('crucible:switch-model', onSwitchModelEvent));

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
        const result = await executeCommand(s.id, message);
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

  // Shift+Tab cycles chat mode (Normal → Plan → Auto). Enter-to-send is the
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

  return (
    <form
      ref={formRef}
      onSubmit={handleSubmit}
      class="border-t border-hairline p-3"
      data-testid="chat-input-form"
    >
      <Show when={error()}>
        <div class="mb-2 px-2 py-1 text-sm text-error bg-error-dark/20 rounded">
          {error()}
        </div>
      </Show>

      {/* No "no active session" notice here — MessageList already renders
          the full empty state above; repeating it in the input strip read
          as two stacked prompts. */}

      <ComposerCard
        value={input}
        setValue={setInput}
        kilnPath={() => currentSession()?.kiln}
        placeholder={session() ? 'Type a message...' : 'Select a session first...'}
        disabled={!session() || isLoading()}
        testid="chat-input"
        onSubmit={() => void handleSubmit()}
        onKeyDown={handleKeyDown}
        chips={
          <>
            <ChipSelect
              name="model"
              options={availableModels().map((m) => ({ value: m, label: formatModelDisplay(m) }))}
              value={currentSession()?.agent_model ?? ''}
              onSelect={handleModelSelect}
              placeholder={currentModel()}
              disabled={!session() || isLoading()}
              testid="model-picker-button"
              optionTestidPrefix="model-option"
            />
            <ChatModeControl />
          </>
        }
        action={
          <Show
            when={isStreaming()}
            fallback={
              <button
                type="submit"
                disabled={!canSend()}
                aria-label="Send message"
                classList={{
                  'px-2.5 flex items-center justify-center transition-colors': true,
                  'bg-primary text-white hover:bg-primary-hover': !!canSend(),
                  'bg-transparent text-muted-dark cursor-not-allowed': !canSend(),
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
              class="px-2.5 flex items-center justify-center bg-error text-white hover:bg-error-dark transition-colors"
              data-testid="cancel-button"
            >
              <X class="w-4 h-4" />
            </button>
          </Show>
        }
      />

      {/* Session scope BELOW the box (launchpad layout): the kilns the
          session knows and the workspace it acts in — attach/detach
          mid-session (Crucible Shell design 4a/5a). */}
      <SessionScopeChips />
    </form>
  );
};
