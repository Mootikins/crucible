<script lang="ts">
  import { sendChatMessage, type ChatMessage, type ChatEvent } from './sse';
  import { renderMarkdown } from './markdown';

  let messages: ChatMessage[] = $state([]);
  let input = $state('');
  let isLoading = $state(false);
  let messagesContainer: HTMLDivElement;

  function scrollToBottom() {
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!input.trim() || isLoading) return;

    const userMessage = input.trim();
    input = '';
    isLoading = true;

    // Add user message
    messages = [...messages, {
      id: crypto.randomUUID(),
      role: 'user',
      content: userMessage
    }];

    // Add placeholder for assistant response
    const assistantId = crypto.randomUUID();
    messages = [...messages, {
      id: assistantId,
      role: 'assistant',
      content: '',
      isStreaming: true
    }];

    scrollToBottom();

    try {
      await sendChatMessage(userMessage, (event: ChatEvent) => {
        // Find and update the assistant message
        messages = messages.map(msg => {
          if (msg.id !== assistantId) return msg;

          switch (event.type) {
            case 'token':
              return { ...msg, content: msg.content + (event.content || '') };
            case 'tool_call':
              return {
                ...msg,
                toolCalls: [...(msg.toolCalls || []), { id: event.id!, title: event.title! }]
              };
            case 'message_complete':
              return {
                ...msg,
                content: event.content || msg.content,
                toolCalls: event.tool_calls || msg.toolCalls,
                isStreaming: false
              };
            case 'error':
              return {
                ...msg,
                content: `Error: ${event.message}`,
                isStreaming: false
              };
            default:
              return msg;
          }
        });
        scrollToBottom();
      });
    } catch (error) {
      // Update assistant message with error
      messages = messages.map(msg =>
        msg.id === assistantId
          ? { ...msg, content: `Error: ${error}`, isStreaming: false }
          : msg
      );
    } finally {
      isLoading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }
</script>

<div class="chat-container">
  <div class="messages" bind:this={messagesContainer}>
    {#each messages as message (message.id)}
      <div class="message {message.role}">
        <div class="message-header">
          {message.role === 'user' ? 'You' : 'Assistant'}
          {#if message.isStreaming}
            <span class="streaming-indicator">...</span>
          {/if}
        </div>
        <div class="message-content">
          {#if message.role === 'assistant'}
            {@html renderMarkdown(message.content)}
          {:else}
            <p>{message.content}</p>
          {/if}
        </div>
        {#if message.toolCalls?.length}
          <div class="tool-calls">
            {#each message.toolCalls as tool}
              <span class="tool-badge">{tool.title}</span>
            {/each}
          </div>
        {/if}
      </div>
    {/each}

    {#if messages.length === 0}
      <div class="empty-state">
        <p>Start a conversation with the AI assistant.</p>
      </div>
    {/if}
  </div>

  <form class="input-area" onsubmit={handleSubmit}>
    <textarea
      bind:value={input}
      onkeydown={handleKeydown}
      placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
      disabled={isLoading}
      rows="3"
    ></textarea>
    <button type="submit" disabled={isLoading || !input.trim()}>
      {isLoading ? 'Sending...' : 'Send'}
    </button>
  </form>
</div>

<style>
  .chat-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    width: 100%;
    max-width: 800px;
    margin: 0 auto;
    padding: 1rem;
    box-sizing: border-box;
    min-height: 0;
  }

  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-height: 0;
  }

  .message {
    padding: 1rem;
    border-radius: 8px;
    max-width: 85%;
  }

  .message.user {
    align-self: flex-end;
    background: #e3f2fd;
  }

  .message.assistant {
    align-self: flex-start;
    background: #f5f5f5;
  }

  .message-header {
    font-size: 0.75rem;
    font-weight: 600;
    margin-bottom: 0.5rem;
    color: #666;
  }

  .streaming-indicator {
    animation: blink 1s infinite;
  }

  @keyframes blink {
    0%, 50% { opacity: 1; }
    51%, 100% { opacity: 0; }
  }

  .message-content {
    line-height: 1.5;
  }

  .message-content :global(pre) {
    background: #1e1e1e;
    padding: 1rem;
    border-radius: 4px;
    overflow-x: auto;
  }

  .message-content :global(code) {
    font-family: 'Fira Code', 'Consolas', monospace;
    font-size: 0.9em;
  }

  .message-content :global(p) {
    margin: 0.5rem 0;
  }

  .tool-calls {
    margin-top: 0.5rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }

  .tool-badge {
    font-size: 0.7rem;
    padding: 0.125rem 0.5rem;
    background: #e0e0e0;
    border-radius: 4px;
    color: #333;
  }

  .empty-state {
    text-align: center;
    color: #999;
    margin-top: 2rem;
  }

  .input-area {
    display: flex;
    gap: 0.5rem;
    padding: 1rem;
    border-top: 1px solid #e0e0e0;
  }

  textarea {
    flex: 1;
    padding: 0.75rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    resize: none;
    font-family: inherit;
    font-size: 1rem;
  }

  textarea:focus {
    outline: none;
    border-color: #2196f3;
  }

  button {
    padding: 0.75rem 1.5rem;
    background: #2196f3;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 1rem;
  }

  button:hover:not(:disabled) {
    background: #1976d2;
  }

  button:disabled {
    background: #ccc;
    cursor: not-allowed;
  }
</style>
