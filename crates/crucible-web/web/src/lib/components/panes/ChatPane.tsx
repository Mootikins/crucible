import { createSignal, For, onMount, type Component } from 'solid-js'
import { sendChatMessage, type ChatMessage, type ChatEvent } from '~/lib/sse'
import { renderMarkdown } from '~/lib/markdown'
import { Button } from '~/lib/components/ui/button'
import { Textarea } from '~/lib/components/ui/textarea'
import { ScrollArea } from '~/lib/components/ui/scroll-area'
import { Badge } from '~/lib/components/ui/badge'
import { Separator } from '~/lib/components/ui/separator'
import { Send } from 'lucide-solid'
import { cn } from '~/lib/utils'

export const ChatPane: Component = () => {
  const [messages, setMessages] = createSignal<ChatMessage[]>([])
  const [input, setInput] = createSignal('')
  const [isLoading, setIsLoading] = createSignal(false)
  let messagesContainer: HTMLDivElement | undefined

  const scrollToBottom = () => {
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight
    }
  }

  const handleSubmit = async (e: Event) => {
    e.preventDefault()
    if (!input().trim() || isLoading()) return

    const userMessage = input().trim()
    setInput('')
    setIsLoading(true)

    // Add user message
    setMessages((prev) => [
      ...prev,
      {
        id: crypto.randomUUID(),
        role: 'user',
        content: userMessage,
      },
    ])

    // Add placeholder for assistant response
    const assistantId = crypto.randomUUID()
    setMessages((prev) => [
      ...prev,
      {
        id: assistantId,
        role: 'assistant',
        content: '',
        isStreaming: true,
      },
    ])

    scrollToBottom()

    try {
      await sendChatMessage(userMessage, (event: ChatEvent) => {
        setMessages((prev) =>
          prev.map((msg) => {
            if (msg.id !== assistantId) return msg

            switch (event.type) {
              case 'token':
                return { ...msg, content: msg.content + (event.content || '') }
              case 'tool_call':
                return {
                  ...msg,
                  toolCalls: [...(msg.toolCalls || []), { id: event.id!, title: event.title! }],
                }
              case 'message_complete':
                return {
                  ...msg,
                  content: event.content || msg.content,
                  toolCalls: event.tool_calls || msg.toolCalls,
                  isStreaming: false,
                }
              case 'error':
                return {
                  ...msg,
                  content: `Error: ${event.message}`,
                  isStreaming: false,
                }
              default:
                return msg
            }
          })
        )
        scrollToBottom()
      })
    } catch (error) {
      // Update assistant message with error
      setMessages((prev) =>
        prev.map((msg) =>
          msg.id === assistantId
            ? { ...msg, content: `Error: ${error}`, isStreaming: false }
            : msg
        )
      )
    } finally {
      setIsLoading(false)
    }
  }

  const handleKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSubmit(e)
    }
  }

  return (
    <div class="flex flex-col h-full w-full min-h-0">
      <ScrollArea class="flex-1 min-h-0">
        <div ref={messagesContainer} class="flex flex-col gap-1.5 p-2">
          <For each={messages()}>
            {(message) => (
              <div
                class={cn(
                  'px-3 py-2 rounded-md max-w-[85%] flex flex-col gap-1',
                  message.role === 'user'
                    ? 'self-end bg-primary/10 border border-primary/20'
                    : 'self-start bg-muted border border-border/30'
                )}
              >
                <div class="flex items-center gap-2 text-xs font-semibold text-muted-foreground">
                  <span>{message.role === 'user' ? 'You' : 'Assistant'}</span>
                  {message.isStreaming && (
                    <span class="animate-pulse">...</span>
                  )}
                </div>
                <div class="text-sm leading-relaxed">
                  {message.role === 'assistant' ? (
                    <div innerHTML={renderMarkdown(message.content)} />
                  ) : (
                    <p>{message.content}</p>
                  )}
                </div>
                {message.toolCalls && message.toolCalls.length > 0 && (
                  <div class="mt-1 flex flex-wrap gap-1">
                    <For each={message.toolCalls}>
                      {(tool) => (
                        <Badge variant="secondary" class="text-xs px-1.5 py-0.5">
                          {tool.title}
                        </Badge>
                      )}
                    </For>
                  </div>
                )}
              </div>
            )}
          </For>
          {messages().length === 0 && (
            <div class="text-center text-muted-foreground mt-8 text-sm">
              <p>Start a conversation with the AI assistant.</p>
            </div>
          )}
        </div>
      </ScrollArea>

      <Separator />

      <form class="flex gap-2 p-2 items-end" onSubmit={handleSubmit}>
        <Textarea
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={handleKeydown}
          placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
          disabled={isLoading()}
          rows={3}
          class="flex-1 text-sm min-h-[2.5rem] resize-none"
        />
        <Button type="submit" disabled={isLoading() || !input().trim()} size="icon" class="h-10 w-10 flex-shrink-0">
          <Send size={14} />
        </Button>
      </form>
    </div>
  )
}

