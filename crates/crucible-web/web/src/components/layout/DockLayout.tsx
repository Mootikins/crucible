import { Component, ParentComponent, createSignal, onMount, onCleanup } from 'solid-js';
import { DockView, DockPanel } from 'solid-dockview';
import type { DockviewComponent, SerializedDockview } from 'dockview-core';
import { SettingsPanel } from '@/components/SettingsPanel';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import { BreadcrumbNav } from '@/components/BreadcrumbNav';
import { loadLayout, saveLayout, type LayoutState } from '@/lib/layout';
import 'dockview-core/dist/styles/dockview.css';

const GearIcon: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-5 h-5">
    <path fill-rule="evenodd" d="M11.078 2.25c-.917 0-1.699.663-1.85 1.567L9.05 4.889c-.02.12-.115.26-.297.348a7.493 7.493 0 00-.986.57c-.166.115-.334.126-.45.083L6.3 5.508a1.875 1.875 0 00-2.282.819l-.922 1.597a1.875 1.875 0 00.432 2.385l.84.692c.095.078.17.229.154.43a7.598 7.598 0 000 1.139c.015.2-.059.352-.153.43l-.841.692a1.875 1.875 0 00-.432 2.385l.922 1.597a1.875 1.875 0 002.282.818l1.019-.382c.115-.043.283-.031.45.082.312.214.641.405.985.57.182.088.277.228.297.35l.178 1.071c.151.904.933 1.567 1.85 1.567h1.844c.916 0 1.699-.663 1.85-1.567l.178-1.072c.02-.12.114-.26.297-.349.344-.165.673-.356.985-.57.167-.114.335-.125.45-.082l1.02.382a1.875 1.875 0 002.28-.819l.923-1.597a1.875 1.875 0 00-.432-2.385l-.84-.692c-.095-.078-.17-.229-.154-.43a7.614 7.614 0 000-1.139c-.016-.2.059-.352.153-.43l.84-.692c.708-.582.891-1.59.433-2.385l-.922-1.597a1.875 1.875 0 00-2.282-.818l-1.02.382c-.114.043-.282.031-.449-.083a7.49 7.49 0 00-.985-.57c-.183-.087-.277-.227-.297-.348l-.179-1.072a1.875 1.875 0 00-1.85-1.567h-1.843zM12 15.75a3.75 3.75 0 100-7.5 3.75 3.75 0 000 7.5z" clip-rule="evenodd" />
  </svg>
);

const SidebarIcon: Component<{ side: 'left' | 'right' }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path
      fill-rule="evenodd"
      d={props.side === 'left'
        ? "M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zM2 10a.75.75 0 01.75-.75h7.5a.75.75 0 010 1.5h-7.5A.75.75 0 012 10zm0 5.25a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75z"
        : "M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zm7.5 5.25a.75.75 0 01.75-.75h7a.75.75 0 010 1.5h-7a.75.75 0 01-.75-.75zM2 15.25a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75a.75.75 0 01-.75-.75z"
      }
      clip-rule="evenodd"
    />
  </svg>
);

const BottomPanelIcon: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path fill-rule="evenodd" d="M2 4.75A.75.75 0 012.75 4h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 4.75zM2 10a.75.75 0 01.75-.75h14.5a.75.75 0 010 1.5H2.75A.75.75 0 012 10zm0 5.25a.75.75 0 01.75-.75h7.5a.75.75 0 010 1.5h-7.5A.75.75 0 012 15.25z" clip-rule="evenodd" />
  </svg>
);

export const ChatPanel: ParentComponent = (props) => (
  <div class="h-full flex flex-col bg-neutral-900">{props.children}</div>
);

export const BottomPanel: Component = () => (
  <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
    <div class="text-center">
      <div class="text-2xl mb-1">📋</div>
      <div class="text-xs">Output / Terminal</div>
    </div>
  </div>
);

interface DockLayoutProps {
  chatContent: Component;
}

interface PanelState {
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
   const [panelVisible, setPanelVisible] = createSignal<PanelState>({
     left: true,
     right: true,
     bottom: false,
   });
   const [ariaLiveMessage, setAriaLiveMessage] = createSignal('');

   const [groupIds, setGroupIds] = createSignal<Record<string, string | null>>({
    left: null,
    right: null,
    bottom: null,
  });

  const debouncedSave = debounce(() => {
    const api = dockviewApi();
    if (!api) return;
    
    const serialized = api.toJSON() as SerializedDockview;
    const layoutState: LayoutState = {
      grid: serialized,
      panels: {
        left: { visible: panelVisible().left },
        right: { visible: panelVisible().right },
        bottom: { visible: panelVisible().bottom },
      },
    };
    saveLayout(layoutState);
  }, 300);

   const togglePanel = (panel: keyof PanelState) => {
     const api = dockviewApi();
     if (!api) return;

     const newVisible = !panelVisible()[panel];
     setPanelVisible(prev => ({ ...prev, [panel]: newVisible }));

     const groups = groupIds();
     const groupId = groups[panel];
     if (groupId) {
       const group = api.getGroup(groupId);
       group?.api.setVisible(newVisible);
     }

     // Announce state change for screen readers
     const panelNames: Record<keyof PanelState, string> = {
       left: 'Left panel',
       right: 'Right panel',
       bottom: 'Bottom panel',
     };
     const message = `${panelNames[panel]} ${newVisible ? 'expanded' : 'collapsed'}`;
     setAriaLiveMessage(message);

     debouncedSave();
   };

   const handleReady = (event: { dockview: DockviewComponent }) => {
     const api = event.dockview;
     setDockviewApi(api);

     const savedLayout = loadLayout();
     if (savedLayout?.grid) {
       try {
         const serialized = savedLayout.grid as SerializedDockview;
         if (serialized.panels && Object.keys(serialized.panels).length > 0) {
           try {
             api.fromJSON(serialized);
           } catch (jsonError) {
             // fromJSON failed - likely due to missing panels or corrupt state
             console.warn('Failed to restore layout from JSON:', jsonError);
             // Clear the corrupt layout and use default
             localStorage.removeItem('crucible:layout');
             // Reset to default state
             setPanelVisible({
               left: true,
               right: true,
               bottom: false,
             });
             setGroupIds({
               left: null,
               right: null,
               bottom: null,
             });
             return;
           }
           
           if (savedLayout.panels) {
             setPanelVisible({
               left: savedLayout.panels.left?.visible !== false,
               right: savedLayout.panels.right?.visible !== false,
               bottom: savedLayout.panels.bottom?.visible === true,
             });
           }
           
           const newGroupIds: Record<string, string | null> = {
             left: null,
             right: null,
             bottom: null,
           };
           
           for (const panel of api.panels) {
             if (panel.id === 'sessions' || panel.id === 'files') {
               newGroupIds.left = panel.group?.id ?? null;
             }
             if (panel.id === 'editor') {
               newGroupIds.right = panel.group?.id ?? null;
             }
             if (panel.id === 'bottom') {
               newGroupIds.bottom = panel.group?.id ?? null;
             }
           }
           
           setGroupIds(newGroupIds);
           
           return;
         }
       } catch (e) {
         console.warn('Failed to restore layout:', e);
         localStorage.removeItem('crucible:layout');
       }
     }
   };

  const trackGroup = (panelId: string, groupId: string | null) => {
    setGroupIds(prev => {
      const updated = { ...prev };
      if (panelId === 'sessions' || panelId === 'files') updated.left = groupId;
      if (panelId === 'editor') updated.right = groupId;
      if (panelId === 'bottom') updated.bottom = groupId;
      return updated;
    });
  };

   onMount(() => {
     const checkApi = setInterval(() => {
       const api = dockviewApi();
       if (api) {
         clearInterval(checkApi);
         const disposable = api.onDidLayoutChange(() => debouncedSave());
         onCleanup(() => disposable.dispose());
       }
     }, 100);
     onCleanup(() => clearInterval(checkApi));

     // Keyboard shortcuts for panel toggles
     const handleKeyDown = (event: KeyboardEvent) => {
       // Don't trigger shortcuts when input/textarea is focused
       if (event.target instanceof HTMLInputElement || 
           event.target instanceof HTMLTextAreaElement ||
           (event.target instanceof HTMLElement && event.target.contentEditable === 'true')) {
         return;
       }

       const isMac = navigator.platform.includes('Mac');
       const modifier = isMac ? event.metaKey : event.ctrlKey;

       if (!modifier) return;

       // Cmd+B / Ctrl+B: Toggle left panel
       if (event.key === 'b' && !event.shiftKey) {
         event.preventDefault();
         togglePanel('left');
       }
       // Cmd+Shift+B / Ctrl+Shift+B: Toggle right panel
       else if (event.key === 'B' && event.shiftKey) {
         event.preventDefault();
         togglePanel('right');
       }
       // Cmd+J / Ctrl+J: Toggle bottom panel
       else if (event.key === 'j') {
         event.preventDefault();
         togglePanel('bottom');
       }
     };

     document.addEventListener('keydown', handleKeyDown);
     onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
   });

  return (
    <div class="h-screen w-screen flex flex-col bg-neutral-950">
      <BreadcrumbNav />

      <div class="flex-1 flex overflow-hidden">
         <div class="flex flex-col justify-center border-r border-neutral-800 bg-neutral-900">
            <button
              data-testid="toggle-left"
              onClick={() => togglePanel('left')}
              aria-label="Toggle left sidebar"
              aria-expanded={panelVisible().left}
              aria-controls="sessions"
              class={`p-2 transition-colors ${panelVisible().left ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
              title="Toggle left sidebar (⌘B)"
            >
              <SidebarIcon side="left" />
            </button>
         </div>

        <div class="flex-1 flex flex-col overflow-hidden">
          <div class="flex-1 overflow-hidden relative">
             <button
               data-testid="toggle-settings"
               onClick={() => setShowSettings(!showSettings())}
               aria-label="Toggle settings panel"
               aria-expanded={showSettings()}
               aria-controls="settings"
               class="absolute top-2 right-2 z-50 p-2 rounded-lg bg-neutral-800/80 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors"
               title="Settings"
             >
               <GearIcon />
             </button>

            <DockView
              class="dockview-theme-abyss"
              style="height: 100%; width: 100%;"
              onReady={handleReady}
              onDidLayoutChange={debouncedSave}
            >
              <DockPanel 
                id="sessions" 
                title="Sessions" 
                position={{ direction: 'left' }}
                initialWidth={260}
                onCreate={(e) => trackGroup('sessions', e.panel.group?.id ?? null)}
              >
                <SessionPanel />
              </DockPanel>

              <DockPanel 
                id="files" 
                title="Files" 
                position={{ referencePanel: 'sessions', direction: 'within' }}
                onCreate={(e) => trackGroup('files', e.panel.group?.id ?? null)}
              >
                <FilesPanel />
              </DockPanel>

              <DockPanel id="chat" title="Chat">
                <props.chatContent />
              </DockPanel>

              <DockPanel 
                id="editor" 
                title="Editor" 
                position={{ direction: 'right' }}
                initialWidth={400}
                onCreate={(e) => trackGroup('editor', e.panel.group?.id ?? null)}
              >
                <EditorPanel />
              </DockPanel>

               <DockPanel 
                 id="bottom" 
                 title="Output" 
                 position={{ direction: 'below' }}
                 initialHeight={200}
                 onCreate={(e) => {
                   trackGroup('bottom', e.panel.group?.id ?? null);
                   const api = dockviewApi();
                   if (api && e.panel.group?.id) {
                     api.getGroup(e.panel.group.id)?.api.setVisible(false);
                   }
                 }}
               >
                 <BottomPanel />
               </DockPanel>

               <DockPanel id="settings" title="Settings" floating={{ width: 400, height: 300 }}>
                 <SettingsPanel />
               </DockPanel>
            </DockView>
          </div>

            <div class="flex justify-center border-t border-neutral-800 bg-neutral-900">
              <button
                data-testid="toggle-bottom"
                onClick={() => togglePanel('bottom')}
                aria-label="Toggle bottom panel"
                aria-expanded={panelVisible().bottom}
                aria-controls="bottom"
                class={`p-1.5 transition-colors ${panelVisible().bottom ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
                title="Toggle bottom panel (⌘J)"
              >
                <BottomPanelIcon />
              </button>
            </div>
        </div>

          <div class="flex flex-col justify-center border-l border-neutral-800 bg-neutral-900">
            <button
              data-testid="toggle-right"
              onClick={() => togglePanel('right')}
              aria-label="Toggle right sidebar"
              aria-expanded={panelVisible().right}
              aria-controls="editor"
              class={`p-2 transition-colors ${panelVisible().right ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
              title="Toggle right sidebar (⌘⇧B)"
            >
              <SidebarIcon side="right" />
            </button>
           </div>
       </div>

       <div
         aria-live="polite"
         aria-atomic="true"
         class="sr-only"
         role="status"
       >
         {ariaLiveMessage()}
       </div>
     </div>
   );
 };
