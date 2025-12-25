import { Component, createSignal } from 'solid-js';
import { useChat } from '@/contexts/ChatContext';
import { MicButton } from './MicButton';

export const ChatInput: Component = () => {
  const { sendMessage, isLoading } = useChat();
  const [input, setInput] = createSignal('');

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    const message = input().trim();
    if (!message || isLoading()) return;

    setInput('');
    await sendMessage(message);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    // Submit on Enter (without Shift)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  const handleTranscription = (text: string) => {
    // Insert transcription at cursor or append
    setInput((prev) => {
      if (prev.trim()) {
        return prev + ' ' + text;
      }
      return text;
    });
  };

  return (
    <form
      onSubmit={handleSubmit}
      class="border-t border-neutral-800 p-4"
      data-testid="chat-input-form"
    >
      <div class="flex items-end gap-2 bg-neutral-900 rounded-xl p-2">
        <textarea
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message..."
          disabled={isLoading()}
          rows={1}
          class="flex-1 bg-transparent text-neutral-100 placeholder-neutral-500 resize-none outline-none px-2 py-1 max-h-32 min-h-[2.5rem]"
          data-testid="chat-input"
        />

        <MicButton
          onTranscription={handleTranscription}
          disabled={isLoading()}
        />

        <button
          type="submit"
          disabled={isLoading() || !input().trim()}
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
      </div>
    </form>
  );
};
