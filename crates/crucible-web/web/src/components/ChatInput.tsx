import { Component, createSignal, Show, For, createEffect, onCleanup } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useMediaRecorder } from '@/hooks/useMediaRecorder';
import { MicButton } from './MicButton';

export const ChatInput: Component = () => {
  const { sendMessage, isLoading, isStreaming, cancelStream, error } = useChatSafe();
  const { currentSession, cancelCurrentOperation, availableModels, switchModel, refreshModels } = useSessionSafe();
  const [input, setInput] = createSignal('');
  const [isModelPickerOpen, setIsModelPickerOpen] = createSignal(false);
  const { isRecording, audioLevel, startRecording, stopRecording } = useMediaRecorder();
  let modelPickerRef: HTMLDivElement | undefined;

  const session = () => currentSession();
  const canSend = () => {
    const s = session();
    return s && s.state === 'active' && !isLoading() && input().trim().length > 0;
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    const message = input().trim();
    if (!message || !canSend()) return;

    setInput('');
    await sendMessage(message);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
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

  // Refresh models when session changes
  createEffect(() => {
    if (currentSession()) {
      refreshModels();
    }
  });

  // Close dropdown when clicking outside
  createEffect(() => {
    if (!isModelPickerOpen()) return;
    
    const handleClickOutside = (e: MouseEvent) => {
      if (modelPickerRef && !modelPickerRef.contains(e.target as Node)) {
        setIsModelPickerOpen(false);
      }
    };
    
    document.addEventListener('mousedown', handleClickOutside);
    onCleanup(() => document.removeEventListener('mousedown', handleClickOutside));
  });

  const currentModel = () => {
    const s = currentSession();
    return s?.agent_model ?? 'Select model';
  };

  const handleModelSelect = async (model: string) => {
    setIsModelPickerOpen(false);
    await switchModel(model);
  };

  const truncateModel = (model: string, maxLen = 20) => {
    if (model.length <= maxLen) return model;
    return model.slice(0, maxLen - 1) + '…';
  };

  const containerStyle = () => {
    if (!isRecording()) return {};
    return {
      background: `linear-gradient(to top,
        rgba(59, 130, 246, 0.4) 0%,
        rgba(59, 130, 246, 0.2) ${fillPercent()}%,
        transparent ${fillPercent()}%)`,
      'border-color': 'rgba(59, 130, 246, 0.6)',
    };
  };

  return (
    <form
      onSubmit={handleSubmit}
      class="border-t border-neutral-800 p-4"
      data-testid="chat-input-form"
    >
      <Show when={error()}>
        <div class="mb-2 px-2 py-1 text-sm text-red-400 bg-red-900/20 rounded">
          {error()}
        </div>
      </Show>
      
      <Show when={!session()}>
        <div class="mb-2 px-2 py-1 text-sm text-neutral-500 text-center">
          No active session. Create or select a session to start chatting.
        </div>
      </Show>

      <div
        class="relative flex flex-col gap-2 bg-neutral-900 rounded-xl p-2 border-2 border-transparent transition-[border-color]"
        style={containerStyle()}
      >
        <textarea
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          placeholder={session() ? "Type a message..." : "Select a session first..."}
          disabled={!session() || isLoading()}
          rows={1}
          class="flex-1 bg-transparent text-neutral-100 placeholder-neutral-500 resize-none outline-none px-2 py-1 max-h-32 min-h-[2.5rem] disabled:opacity-50"
          data-testid="chat-input"
        />

        <div class="flex items-center gap-2">
          <div ref={modelPickerRef} class="relative">
            <button
              type="button"
              onClick={() => setIsModelPickerOpen(!isModelPickerOpen())}
              disabled={!session() || isLoading()}
              class="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium text-neutral-300 bg-neutral-800 hover:bg-neutral-700 rounded-lg border border-neutral-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              data-testid="model-picker-button"
            >
              <span class="max-w-[140px] truncate">{truncateModel(currentModel())}</span>
              <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 20 20"
                fill="currentColor"
                class="w-3.5 h-3.5 transition-transform"
                classList={{ 'rotate-180': isModelPickerOpen() }}
              >
                <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
              </svg>
            </button>

            <Show when={isModelPickerOpen()}>
              <div class="absolute bottom-full left-0 mb-1 w-56 max-h-64 overflow-y-auto bg-neutral-800 border border-neutral-700 rounded-lg shadow-xl z-50">
                <Show
                  when={availableModels().length > 0}
                  fallback={
                    <div class="px-3 py-2 text-xs text-neutral-500">No models available</div>
                  }
                >
                  <For each={availableModels()}>
                    {(model) => (
                      <button
                        type="button"
                        onClick={() => handleModelSelect(model)}
                        class="w-full px-3 py-2 text-left text-sm text-neutral-200 hover:bg-neutral-700 transition-colors first:rounded-t-lg last:rounded-b-lg"
                        classList={{ 'bg-neutral-700/50': model === currentSession()?.agent_model }}
                      >
                        <span class="flex items-center gap-2">
                          <Show when={model === currentSession()?.agent_model}>
                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 text-blue-400">
                              <path fill-rule="evenodd" d="M16.704 4.153a.75.75 0 01.143 1.052l-8 10.5a.75.75 0 01-1.127.075l-4.5-4.5a.75.75 0 011.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 011.05-.143z" clip-rule="evenodd" />
                            </svg>
                          </Show>
                          <span class="truncate">{model}</span>
                        </span>
                      </button>
                    )}
                  </For>
                </Show>
              </div>
            </Show>
          </div>

          <div class="flex-1" />

          <MicButton
            onTranscription={handleTranscription}
            disabled={!session() || isLoading()}
            startRecording={startRecording}
            stopRecording={stopRecording}
            isRecording={isRecording}
          />

          <Show
            when={isStreaming()}
            fallback={
              <button
                type="submit"
                disabled={!canSend()}
                class="p-2 rounded-lg bg-blue-600 text-white disabled:opacity-50 disabled:cursor-not-allowed hover:bg-blue-700 transition-colors"
                data-testid="send-button"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  class="w-5 h-5"
                >
                  <path d="M3.478 2.405a.75.75 0 00-.926.94l2.432 7.905H13.5a.75.75 0 010 1.5H4.984l-2.432 7.905a.75.75 0 00.926.94 60.519 60.519 0 0018.445-8.986.75.75 0 000-1.218A60.517 60.517 0 003.478 2.405z" />
                </svg>
              </button>
            }
          >
            <button
              type="button"
              onClick={handleCancel}
              class="p-2 rounded-lg bg-red-600 text-white hover:bg-red-700 transition-colors"
              data-testid="cancel-button"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 24 24"
                fill="currentColor"
                class="w-5 h-5"
              >
                <path fill-rule="evenodd" d="M5.47 5.47a.75.75 0 011.06 0L12 10.94l5.47-5.47a.75.75 0 111.06 1.06L13.06 12l5.47 5.47a.75.75 0 11-1.06 1.06L12 13.06l-5.47 5.47a.75.75 0 01-1.06-1.06L10.94 12 5.47 6.53a.75.75 0 010-1.06z" clip-rule="evenodd" />
              </svg>
            </button>
          </Show>
        </div>
      </div>
    </form>
  );
};
