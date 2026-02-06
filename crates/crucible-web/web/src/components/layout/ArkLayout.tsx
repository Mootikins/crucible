import { Component, ParentComponent, createSignal, createEffect, onMount, onCleanup, Show } from 'solid-js';
import { Splitter, Tabs, Dialog } from '@ark-ui/solid';
import type { PanelSizeData, SizeChangeDetails } from '@zag-js/splitter';
import { Portal } from 'solid-js/web';
import { SettingsPanel } from '@/components/SettingsPanel';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { EditorPanel } from '@/components/EditorPanel';
import { BreadcrumbNav } from '@/components/BreadcrumbNav';

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

const CloseIcon: Component = () => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
    <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
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

interface ArkLayoutProps {
  chatContent: Component;
}

interface ArkLayoutState {
  horizontalSizes: PanelSizeData[];
  verticalSizes: PanelSizeData[];
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  bottomCollapsed: boolean;
  centerTab: string;
}

const STORAGE_KEY = 'crucible:ark-layout';

const DEFAULT_HORIZONTAL_SIZES: PanelSizeData[] = [
  { id: 'left', size: 20, minSize: 15, maxSize: 30 },
  { id: 'center', size: 60, minSize: 40 },
  { id: 'right', size: 20, minSize: 15, maxSize: 30 },
];

const DEFAULT_VERTICAL_SIZES: PanelSizeData[] = [
  { id: 'top', size: 75, minSize: 30 },
  { id: 'bottom', size: 25, minSize: 10, maxSize: 50 },
];

function loadArkLayout(): ArkLayoutState | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      return JSON.parse(stored);
    }
  } catch (e) {
    console.warn('Failed to load Ark layout:', e);
  }
  return null;
}

function saveArkLayout(state: ArkLayoutState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch (e) {
    console.warn('Failed to save Ark layout:', e);
  }
}

function debounce<T extends (...args: unknown[]) => void>(fn: T, delay: number): T {
  let timeoutId: ReturnType<typeof setTimeout>;
  return ((...args: unknown[]) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

export const ArkLayout: Component<ArkLayoutProps> = (props) => {
  const savedState = loadArkLayout();
  
  const [horizontalSizes, setHorizontalSizes] = createSignal<PanelSizeData[]>(
    savedState?.horizontalSizes ?? DEFAULT_HORIZONTAL_SIZES
  );
  const [verticalSizes, setVerticalSizes] = createSignal<PanelSizeData[]>(
    savedState?.verticalSizes ?? DEFAULT_VERTICAL_SIZES
  );
  
  const [leftCollapsed, setLeftCollapsed] = createSignal(savedState?.leftCollapsed ?? false);
  const [rightCollapsed, setRightCollapsed] = createSignal(savedState?.rightCollapsed ?? false);
  const [bottomCollapsed, setBottomCollapsed] = createSignal(savedState?.bottomCollapsed ?? true);
  
  const [centerTab, setCenterTab] = createSignal(savedState?.centerTab ?? 'chat');
  
  const [showSettings, setShowSettings] = createSignal(false);
  
  const [ariaLiveMessage, setAriaLiveMessage] = createSignal('');

  const debouncedSave = debounce(() => {
    saveArkLayout({
      horizontalSizes: horizontalSizes(),
      verticalSizes: verticalSizes(),
      leftCollapsed: leftCollapsed(),
      rightCollapsed: rightCollapsed(),
      bottomCollapsed: bottomCollapsed(),
      centerTab: centerTab(),
    });
  }, 300);

  createEffect(() => {
    horizontalSizes();
    verticalSizes();
    leftCollapsed();
    rightCollapsed();
    bottomCollapsed();
    centerTab();
    debouncedSave();
  });

  const toggleLeft = () => {
    const newState = !leftCollapsed();
    setLeftCollapsed(newState);
    setAriaLiveMessage(`Left panel ${newState ? 'collapsed' : 'expanded'}`);
  };

  const toggleRight = () => {
    const newState = !rightCollapsed();
    setRightCollapsed(newState);
    setAriaLiveMessage(`Right panel ${newState ? 'collapsed' : 'expanded'}`);
  };

  const toggleBottom = () => {
    const newState = !bottomCollapsed();
    setBottomCollapsed(newState);
    setAriaLiveMessage(`Bottom panel ${newState ? 'collapsed' : 'expanded'}`);
  };

  const handleHorizontalSizeChange = (details: SizeChangeDetails) => {
    setHorizontalSizes(details.size);
  };

  const handleVerticalSizeChange = (details: SizeChangeDetails) => {
    setVerticalSizes(details.size);
  };

  onMount(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || 
          event.target instanceof HTMLTextAreaElement ||
          (event.target instanceof HTMLElement && event.target.contentEditable === 'true')) {
        return;
      }

      const isMac = navigator.platform.includes('Mac');
      const modifier = isMac ? event.metaKey : event.ctrlKey;

      if (!modifier) return;

      if (event.key === 'b' && !event.shiftKey) {
        event.preventDefault();
        toggleLeft();
      } else if (event.key === 'B' && event.shiftKey) {
        event.preventDefault();
        toggleRight();
      } else if (event.key === 'j') {
        event.preventDefault();
        toggleBottom();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  return (
    <div data-testid="ark-layout" class="h-screen w-screen flex flex-col bg-neutral-950">
      <BreadcrumbNav />

      <div class="flex-1 flex overflow-hidden">
        <div class="flex flex-col justify-center border-r border-neutral-800 bg-neutral-900">
          <button
            data-testid="ark-toggle-left"
            onClick={toggleLeft}
            aria-label="Toggle left sidebar"
            aria-expanded={!leftCollapsed()}
            class={`p-2 transition-colors ${!leftCollapsed() ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
            title="Toggle left sidebar (⌘B)"
          >
            <SidebarIcon side="left" />
          </button>
        </div>

        <div class="flex-1 flex flex-col overflow-hidden">
          <Splitter.Root
            orientation="vertical"
            size={verticalSizes()}
            onSizeChange={handleVerticalSizeChange}
            class="flex-1 flex flex-col"
          >
            <Splitter.Panel id="top" class="flex-1 overflow-hidden">
              <Splitter.Root
                orientation="horizontal"
                size={horizontalSizes()}
                onSizeChange={handleHorizontalSizeChange}
                class="h-full flex"
              >
                <Splitter.Panel
                  id="left"
                  class="overflow-hidden transition-all duration-200"
                  style={{ display: leftCollapsed() ? 'none' : undefined }}
                >
                  <Show when={!leftCollapsed()}>
                    <div data-testid="ark-left-panel" class="h-full flex flex-col bg-neutral-900 border-r border-neutral-800">
                      <div class="flex-1 overflow-hidden">
                        <SessionPanel />
                      </div>
                      <div class="border-t border-neutral-800 flex-1 overflow-hidden">
                        <FilesPanel />
                      </div>
                    </div>
                  </Show>
                </Splitter.Panel>

                <Show when={!leftCollapsed()}>
                  <Splitter.ResizeTrigger
                    id="left:center"
                    class="w-1 bg-neutral-800 hover:bg-blue-500 transition-colors cursor-col-resize"
                  />
                </Show>

                <Splitter.Panel id="center" class="overflow-hidden relative flex-1">
                  <div data-testid="ark-center-panel" class="h-full flex flex-col bg-neutral-900">
                    <button
                      data-testid="ark-toggle-settings"
                      onClick={() => setShowSettings(!showSettings())}
                      aria-label="Toggle settings panel"
                      aria-expanded={showSettings()}
                      class="absolute top-2 right-2 z-50 p-2 rounded-lg bg-neutral-800/80 hover:bg-neutral-700 text-neutral-400 hover:text-white transition-colors"
                      title="Settings"
                    >
                      <GearIcon />
                    </button>

                    <Tabs.Root
                      value={centerTab()}
                      onValueChange={(details) => setCenterTab(details.value)}
                      class="h-full flex flex-col"
                    >
                      <Tabs.List class="flex border-b border-neutral-800 bg-neutral-900/50">
                        <Tabs.Trigger
                          value="chat"
                          class="px-4 py-2 text-sm font-medium transition-colors data-[selected]:text-blue-400 data-[selected]:border-b-2 data-[selected]:border-blue-400 text-neutral-400 hover:text-neutral-200"
                        >
                          Chat
                        </Tabs.Trigger>
                        <Tabs.Trigger
                          value="editor"
                          class="px-4 py-2 text-sm font-medium transition-colors data-[selected]:text-blue-400 data-[selected]:border-b-2 data-[selected]:border-blue-400 text-neutral-400 hover:text-neutral-200"
                        >
                          Editor
                        </Tabs.Trigger>
                        <Tabs.Indicator class="bg-blue-400" />
                      </Tabs.List>
                      
                      <Tabs.Content value="chat" class="flex-1 overflow-hidden">
                        <props.chatContent />
                      </Tabs.Content>
                      
                      <Tabs.Content value="editor" class="flex-1 overflow-hidden">
                        <EditorPanel />
                      </Tabs.Content>
                    </Tabs.Root>
                  </div>
                </Splitter.Panel>

                <Show when={!rightCollapsed()}>
                  <Splitter.ResizeTrigger
                    id="center:right"
                    class="w-1 bg-neutral-800 hover:bg-blue-500 transition-colors cursor-col-resize"
                  />
                </Show>

                <Splitter.Panel
                  id="right"
                  class="overflow-hidden transition-all duration-200"
                  style={{ display: rightCollapsed() ? 'none' : undefined }}
                >
                  <Show when={!rightCollapsed()}>
                    <div data-testid="ark-right-panel" class="h-full bg-neutral-900 border-l border-neutral-800">
                      <EditorPanel />
                    </div>
                  </Show>
                </Splitter.Panel>
              </Splitter.Root>
            </Splitter.Panel>

            <Show when={!bottomCollapsed()}>
              <Splitter.ResizeTrigger
                id="top:bottom"
                class="h-1 bg-neutral-800 hover:bg-blue-500 transition-colors cursor-row-resize"
              />
            </Show>

            <Splitter.Panel
              id="bottom"
              class="overflow-hidden transition-all duration-200"
              style={{ display: bottomCollapsed() ? 'none' : undefined }}
            >
              <Show when={!bottomCollapsed()}>
                <div data-testid="ark-bottom-panel" class="h-full bg-neutral-900 border-t border-neutral-800">
                  <BottomPanel />
                </div>
              </Show>
            </Splitter.Panel>
          </Splitter.Root>

          <div class="flex justify-center border-t border-neutral-800 bg-neutral-900">
            <button
              data-testid="ark-toggle-bottom"
              onClick={toggleBottom}
              aria-label="Toggle bottom panel"
              aria-expanded={!bottomCollapsed()}
              class={`p-1.5 transition-colors ${!bottomCollapsed() ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
              title="Toggle bottom panel (⌘J)"
            >
              <BottomPanelIcon />
            </button>
          </div>
        </div>

        <div class="flex flex-col justify-center border-l border-neutral-800 bg-neutral-900">
          <button
            data-testid="ark-toggle-right"
            onClick={toggleRight}
            aria-label="Toggle right sidebar"
            aria-expanded={!rightCollapsed()}
            class={`p-2 transition-colors ${!rightCollapsed() ? 'text-blue-400' : 'text-neutral-500 hover:text-neutral-300'}`}
            title="Toggle right sidebar (⌘⇧B)"
          >
            <SidebarIcon side="right" />
          </button>
        </div>
      </div>

      <Dialog.Root open={showSettings()} onOpenChange={(details) => setShowSettings(details.open)}>
        <Portal>
          <Dialog.Positioner class="fixed inset-0 pointer-events-none z-50">
            <Dialog.Content
              class="pointer-events-auto absolute top-16 right-16 w-[400px] max-h-[80vh] bg-neutral-900 border border-neutral-700 rounded-xl shadow-2xl overflow-hidden"
            >
              <div class="flex items-center justify-between p-4 border-b border-neutral-800">
                <Dialog.Title class="text-lg font-semibold text-white">Settings</Dialog.Title>
                <Dialog.CloseTrigger class="p-1 rounded hover:bg-neutral-800 text-neutral-400 hover:text-white transition-colors">
                  <CloseIcon />
                </Dialog.CloseTrigger>
              </div>
              <Dialog.Description class="sr-only">Application settings panel</Dialog.Description>
              <div class="p-4 overflow-y-auto max-h-[calc(80vh-60px)]">
                <SettingsPanel />
              </div>
            </Dialog.Content>
          </Dialog.Positioner>
        </Portal>
      </Dialog.Root>

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
