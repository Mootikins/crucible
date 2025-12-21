import { type Component, type JSX } from 'solid-js'
import { ResizablePanelGroup, ResizablePanel, ResizableHandle } from '~/lib/components/ui/resizable'
import { ChatPane } from '~/lib/components/panes/ChatPane'
import { DocumentTreePane } from '~/lib/components/panes/DocumentTreePane'
import { DocumentViewPane } from '~/lib/components/panes/DocumentViewPane'
import { layoutStore } from '~/lib/stores/layout'
import { createSignal, createEffect } from 'solid-js'

interface AppLayoutProps {
  sidebar?: JSX.Element
  main?: JSX.Element
  panel?: JSX.Element
}

export const AppLayout: Component<AppLayoutProps> = (props) => {
  const [selectedDocument, setSelectedDocument] = createSignal<string | null>(null)

  // Initialize layout from store
  createEffect(() => {
    const layout = layoutStore.layout()
    // Apply layout state if needed
  })

  return (
    <div class="h-screen w-screen overflow-hidden bg-background flex flex-col">
      <header class="h-10 min-h-10 flex items-center px-2 border-b border-border/30 bg-background flex-shrink-0">
        <h1 class="text-sm font-medium m-0 p-0">Crucible</h1>
      </header>
      <div class="flex-1 overflow-hidden min-h-0">
        <ResizablePanelGroup orientation="horizontal">
          {/* Left Sidebar - Document Tree */}
          <ResizablePanel initialSize={0.2} minSize={0.15} maxSize={0.3}>
            <div class="h-full overflow-hidden border-r border-border/30">
              <DocumentTreePane onFileSelect={setSelectedDocument} />
            </div>
          </ResizablePanel>

          <ResizableHandle />

          {/* Main Content - Document View */}
          <ResizablePanel initialSize={0.55}>
            <div class="h-full overflow-hidden">
              <DocumentViewPane documentPath={selectedDocument()} />
            </div>
          </ResizablePanel>

          <ResizableHandle />

          {/* Right Panel - Chat */}
          <ResizablePanel initialSize={0.25} minSize={0.15} maxSize={0.4}>
            <div class="h-full overflow-hidden border-l border-border/30">
              <ChatPane />
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </div>
  )
}

