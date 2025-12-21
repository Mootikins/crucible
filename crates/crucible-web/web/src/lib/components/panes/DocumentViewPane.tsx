import { createEffect, createSignal, type Component } from 'solid-js'
import { ScrollArea } from '~/lib/components/ui/scroll-area'
import { renderMarkdown } from '~/lib/markdown'

interface DocumentViewPaneProps {
  documentPath?: string | null
  documentContent?: string
}

export const DocumentViewPane: Component<DocumentViewPaneProps> = (props) => {
  const [documentContent, setDocumentContent] = createSignal(props.documentContent ?? '')

  // Mock content for now - will be replaced with real document loading
  createEffect(() => {
    if (props.documentPath) {
      // TODO: Load document content from API
      setDocumentContent(`# ${props.documentPath}\n\nThis is a placeholder document view. Content will be loaded from the API.`)
    }
  })

  return (
    <div class="flex flex-col h-full w-full">
      {props.documentPath ? (
        <>
          <div class="px-3 py-2 border-b border-border/30 bg-background">
            <span class="text-sm font-medium">{props.documentPath}</span>
          </div>
          <ScrollArea>
            <div
              class="p-4 text-sm leading-relaxed markdown-content"
              innerHTML={renderMarkdown(documentContent())}
            />
          </ScrollArea>
        </>
      ) : (
        <div class="flex items-center justify-center h-full text-muted-foreground text-sm">
          <p>Select a document from the tree to view it here.</p>
        </div>
      )}
    </div>
  )
}

