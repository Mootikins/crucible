import { Component, ParentComponent, createSignal, Show, onMount, onCleanup } from 'solid-js';
import { DockView, DockPanel } from 'solid-dockview';
import type { DockviewComponent, SerializedDockview } from 'dockview-core';
import { SettingsPanel } from '@/components/SettingsPanel';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { BreadcrumbNav } from '@/components/BreadcrumbNav';
import { loadLayout, saveLayout, type LayoutState } from '@/lib/layout';
import 'dockview-core/dist/styles/dockview.css';

const GearIcon: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-5 h-5">
    <path fill-rule="evenodd" d="M11.078 2.25c-.917 0-1.699.663-1.85 1.567L9.05 4.889c-.02.12-.115.26-.297.348a7.493 7.493 0 00-.986.57c-.166.115-.334.126-.45.083L6.3 5.508a1.875 1.875 0 00-2.282.819l-.922 1.597a1.875 1.875 0 00.432 2.385l.84.692c.095.078.17.229.154.43a7.598 7.598 0 000 1.139c.015.2-.059.352-.153.43l-.841.692a1.875 1.875 0 00-.432 2.385l.922 1.597a1.875 1.875 0 002.282.818l1.019-.382c.115-.043.283-.031.45.082.312.214.641.405.985.57.182.088.277.228.297.35l.178 1.071c.151.904.933 1.567 1.85 1.567h1.844c.916 0 1.699-.663 1.85-1.567l.178-1.072c.02-.12.114-.26.297-.349.344-.165.673-.356.985-.57.167-.114.335-.125.45-.082l1.02.382a1.875 1.875 0 002.28-.819l.923-1.597a1.875 1.875 0 00-.432-2.385l-.84-.692c-.095-.078-.17-.229-.154-.43a7.614 7.614 0 000-1.139c-.016-.2.059-.352.153-.43l.84-.692c.708-.582.891-1.59.433-2.385l-.922-1.597a1.875 1.875 0 00-2.282-.818l-1.02.382c-.114.043-.282.031-.449-.083a7.49 7.49 0 00-.985-.57c-.183-.087-.277-.227-.297-.348l-.179-1.072a1.875 1.875 0 00-1.85-1.567h-1.843zM12 15.75a3.75 3.75 0 100-7.5 3.75 3.75 0 000 7.5z" clip-rule="evenodd" />
  </svg>
);

const ChevronLeft: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path fill-rule="evenodd" d="M12.79 5.23a.75.75 0 01-.02 1.06L8.832 10l3.938 3.71a.75.75 0 11-1.04 1.08l-4.5-4.25a.75.75 0 010-1.08l4.5-4.25a.75.75 0 011.06.02z" clip-rule="evenodd" />
  </svg>
);

const ChevronRight: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" clip-rule="evenodd" />
  </svg>
);

const ChevronUp: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path fill-rule="evenodd" d="M14.77 12.79a.75.75 0 01-1.06-.02L10 8.832 6.29 12.77a.75.75 0 11-1.08-1.04l4.25-4.5a.75.75 0 011.08 0l4.25 4.5a.75.75 0 01-.02 1.06z" clip-rule="evenodd" />
  </svg>
);

const ChevronDown: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
  </svg>
);

export const ChatPanel: ParentComponent = (props) => {
  return (
    <div class="h-full flex flex-col bg-neutral-900">
      {props.children}
    </div>
  );
};

export const PreviewPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">📄</div>
        <div>Markdown Preview</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

export const EditorPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">✏️</div>
        <div>Editor</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

export const CanvasPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">🎨</div>
        <div>Canvas</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

export const GraphPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">🕸️</div>
        <div>Knowledge Graph</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};



export const BottomPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">📋</div>
        <div>Output / Terminal</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

interface DockLayoutProps {
  chatContent: Component;
}

interface EdgeState {
  left: boolean;
  right: boolean;
  bottom: boolean;
}

function debounce<T extends (...args: unknown[]) => void>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout>;
  return ((...args: unknown[]) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

export const DockLayout: Component<DockLayoutProps> = (props) => {
  const [showSettings, setShowSettings] = createSignal(false);
  const [dockviewApi, setDockviewApi] = createSignal<DockviewComponent | null>(null);
  const [edgeCollapsed, setEdgeCollapsed] = createSignal<EdgeState>({
    left: false,
    right: false,
    bottom: true,
  });

  let leftGroupId: string | null = null;
  let rightGroupId: string | null = null;
  let bottomGroupId: string | null = null;

  const debouncedSave = debounce(() => {
    const api = dockviewApi();
    if (!api) return;
    
    const serialized = api.toJSON() as SerializedDockview;
    const layoutState: LayoutState = {
      grid: serialized,
      panels: {
        files: { visible: !edgeCollapsed().left },
        editor: { visible: true },
        chat: { visible: !edgeCollapsed().right },
        bottom: { visible: !edgeCollapsed().bottom },
      },
    };
    saveLayout(layoutState);
  }, 300);

  const toggleEdge = (edge: keyof EdgeState) => {
    const api = dockviewApi();
    if (!api) return;

    const newState = { ...edgeCollapsed(), [edge]: !edgeCollapsed()[edge] };
    setEdgeCollapsed(newState);

    let groupId: string | null = null;
    switch (edge) {
      case 'left':
        groupId = leftGroupId;
        break;
      case 'right':
        groupId = rightGroupId;
        break;
      case 'bottom':
        groupId = bottomGroupId;
        break;
    }

    if (groupId) {
      const group = api.getGroup(groupId);
      if (group) {
        api.setVisible(group, !newState[edge]);
      }
    }

    debouncedSave();
  };

  const handleReady = (event: { dockview: DockviewComponent }) => {
    const api = event.dockview;
    setDockviewApi(api);

    const savedLayout = loadLayout();
    if (savedLayout?.grid) {
      try {
        api.fromJSON(savedLayout.grid as SerializedDockview);
        
        if (savedLayout.panels) {
          setEdgeCollapsed({
            left: savedLayout.panels.files?.visible === false,
            right: savedLayout.panels.chat?.visible === false,
            bottom: savedLayout.panels.bottom?.visible !== true,
          });
        }
        
        setTimeout(() => {
          const panels = api.panels;
          for (const panel of panels) {
            if (panel.id === 'files') leftGroupId = panel.group?.id ?? null;
            if (panel.id === 'chat') rightGroupId = panel.group?.id ?? null;
            if (panel.id === 'bottom') bottomGroupId = panel.group?.id ?? null;
          }
        }, 0);
        
        return;
      } catch (e) {
        console.warn('Failed to restore layout, using default:', e);
      }
    }
  };

  const handlePanelCreate = (panelId: string, event: { panel: { group?: { id: string } } }) => {
    const groupId = event.panel.group?.id ?? null;
    switch (panelId) {
      case 'files':
        leftGroupId = groupId;
        break;
      case 'chat':
        rightGroupId = groupId;
        break;
      case 'bottom':
        bottomGroupId = groupId;
        break;
    }
  };

  onMount(() => {
    const checkApi = setInterval(() => {
      const api = dockviewApi();
      if (api) {
        clearInterval(checkApi);
        const disposable = api.onDidLayoutChange(() => {
          debouncedSave();
        });

        onCleanup(() => {
          disposable.dispose();
        });
      }
    }, 100);

    onCleanup(() => {
      clearInterval(checkApi);
    });
  });

  return (
    <div class="relative h-screen w-screen flex flex-col">
      <BreadcrumbNav />

      <div class="relative flex-1">
        <button
          onClick={() => setShowSettings(!showSettings())}
          class="absolute top-2 right-2 z-50 p-2 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors"
          title="Settings"
        >
          <GearIcon />
        </button>

      <button
        onClick={() => toggleEdge('left')}
        class="absolute left-0 top-1/2 -translate-y-1/2 z-40 p-1 bg-neutral-800 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors rounded-r-md border border-l-0 border-neutral-700"
        title={edgeCollapsed().left ? 'Expand left panel' : 'Collapse left panel'}
      >
        {edgeCollapsed().left ? <ChevronRight /> : <ChevronLeft />}
      </button>

      <button
        onClick={() => toggleEdge('right')}
        class="absolute right-0 top-1/2 -translate-y-1/2 z-40 p-1 bg-neutral-800 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors rounded-l-md border border-r-0 border-neutral-700"
        title={edgeCollapsed().right ? 'Expand right panel' : 'Collapse right panel'}
      >
        {edgeCollapsed().right ? <ChevronLeft /> : <ChevronRight />}
      </button>

      <button
        onClick={() => toggleEdge('bottom')}
        class="absolute bottom-0 left-1/2 -translate-x-1/2 z-40 p-1 bg-neutral-800 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors rounded-t-md border border-b-0 border-neutral-700"
        title={edgeCollapsed().bottom ? 'Expand bottom panel' : 'Collapse bottom panel'}
      >
        {edgeCollapsed().bottom ? <ChevronUp /> : <ChevronDown />}
      </button>

      <DockView
        class="dockview-theme-abyss"
        style="height: 100%; width: 100%;"
        onReady={handleReady}
        onDidLayoutChange={debouncedSave}
      >
        <DockPanel 
          id="files" 
          title="Files" 
          position={{ direction: 'left' }}
          onCreate={(e) => handlePanelCreate('files', e)}
        >
          <FilesPanel />
        </DockPanel>

        <DockPanel 
          id="editor" 
          title="Editor"
          onCreate={(e) => handlePanelCreate('editor', e)}
        >
          <EditorPanel />
        </DockPanel>

        <DockPanel 
          id="chat" 
          title="Chat" 
          position={{ direction: 'right' }}
          onCreate={(e) => handlePanelCreate('chat', e)}
        >
          <props.chatContent />
        </DockPanel>

        <DockPanel 
          id="bottom" 
          title="Output" 
          position={{ direction: 'below' }}
          onCreate={(e) => {
            handlePanelCreate('bottom', e);
            setTimeout(() => {
              const api = dockviewApi();
              if (api && bottomGroupId) {
                const group = api.getGroup(bottomGroupId);
                if (group) {
                  api.setVisible(group, false);
                }
              }
            }, 0);
          }}
        >
          <BottomPanel />
        </DockPanel>

        <DockPanel 
          id="sessions" 
          title="Sessions" 
          position={{ referencePanel: 'files', direction: 'within' }}
        >
          <SessionPanel />
        </DockPanel>

        <Show when={showSettings()}>
          <DockPanel id="settings" title="Settings" floating={{ width: 400, height: 300 }}>
            <SettingsPanel />
          </DockPanel>
        </Show>
      </DockView>
      </div>
    </div>
  );
};
