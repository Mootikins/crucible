import { Component, createSignal, Show, onCleanup } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useMediaRecorder } from '@/hooks/useMediaRecorder';
import { useAutocomplete } from '@/hooks/useAutocomplete';
import { MicButton } from './MicButton';
import { ChatModeControl, nextChatMode } from './ChatModeControl';
import { AutocompletePopup } from './AutocompletePopup';
import { SessionScopeChips } from './SessionScopeChips';
import { ChipSelect } from '@/components/composer/ChipSelect';
import { executeCommand } from '@/lib/api';
import { statusBarStore } from '@/stores/statusBarStore';
import { ArrowUp, X } from '@/lib/icons';
export const ChatInput: Component = () => {
  const { sessionId, sendMessage, isLoading, isStreaming, cancelStream, error, chatMode, switchMode, addSystemMessage, clearMessages } = useChatSafe();
  const { currentSession, cancelCurrentOperation, availableModels, switchModel } = useSessionSafe();
  const [input, setInput] = createSignal('');
  const { isRecording, audioLevel, startRecording, stopRecording } = useMediaRecorder();
  let formRef: HTMLFormElement | undefined;
  let textareaRef: HTMLTextAreaElement | undefined;

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

  const autocomplete = useAutocomplete({
    input,
    setInput,
    kilnPath: () => currentSession()?.kiln,
    textareaRef: () => textareaRef,
  });

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
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

  const handleKeyDown = (e: KeyboardEvent) => {
    autocomplete.onKeyDown(e);
    if (e.defaultPrevented) return;

    // Shift+Tab cycles chat mode (Normal → Plan → Auto)
    if (e.key === 'Tab' && e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      switchMode(nextChatMode(chatMode()));
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  const handleCancel = async () => {
    cancelStream();
    await cancelCurrentOperation();
  };

  const handleTranscription = (text: string) => {
    setInput((prev) => {
      if (prev.trim()) {
        return prev + ' ' + text;
      }
      return text;
    });
  };

  const fillPercent = () => Math.round(audioLevel() * 100);



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

  const containerStyle = () => {
    if (!isRecording()) return {};
    return {
      background: `linear-gradient(to top,
        color-mix(in srgb, var(--color-primary) 40%, transparent) 0%,
        color-mix(in srgb, var(--color-primary) 20%, transparent) ${fillPercent()}%,
        transparent ${fillPercent()}%)`,
      'border-color': 'color-mix(in srgb, var(--color-primary) 60%, transparent)',
    };
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

      {/* The composer card — same skin as the launchpad (CenterComposer):
          textarea over a footer row of ChipSelect popouts. */}
      <div
        class="relative bg-surface-base border border-hairline-strong rounded-xl px-3 pt-2 pb-2 focus-within:border-primary transition-colors shadow-lg"
        style={containerStyle()}
      >
        <div class="relative">
          <textarea
            ref={textareaRef}
            value={input()}
            onInput={(e) => void autocomplete.onInput(e)}
            onKeyDown={handleKeyDown}
            placeholder={session() ? 'Type a message...' : 'Select a session first...'}
            disabled={!session() || isLoading()}
            rows={1}
            class="w-full bg-transparent text-sm text-shell-ink placeholder-muted-dark resize-none outline-none px-1 py-1 max-h-32 min-h-[2.5rem] disabled:opacity-50"
            data-testid="chat-input"
          />
          <Show when={autocomplete.isOpen()}>
            <AutocompletePopup
              items={autocomplete.items()}
              selectedIndex={autocomplete.selectedIndex()}
              onSelect={(index) => autocomplete.complete(index)}
            />
          </Show>
        </div>

        <div class="flex items-center gap-1">
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

          <div class="flex-1" />

          {/* Mic and send share one pill, split by a hairline. */}
          <div class="flex items-stretch rounded-full border border-hairline overflow-hidden">
            <MicButton
              onTranscription={handleTranscription}
              disabled={!session() || isLoading()}
              startRecording={startRecording}
              stopRecording={stopRecording}
              isRecording={isRecording}
            />
            <div class="w-px bg-hairline" />
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
          </div>
        </div>
      </div>

      {/* Session scope BELOW the box (launchpad layout): the kilns the
          session knows and the workspace it acts in — attach/detach
          mid-session (Crucible Shell design 4a/5a). */}
      <SessionScopeChips />
    </form>
  );
};
