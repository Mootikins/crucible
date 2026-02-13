import { Component, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { createDroppable, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from './TabBar';
import { windowStore, windowActions } from '@/stores/windowStore';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { AppWindow } from '@/lib/icons';

type PaneDropPosition = 'left' | 'right' | 'top' | 'bottom';

function PaneDropZone(props: {
  position: PaneDropPosition;
  droppable: ReturnType<typeof createDroppable>;
  class: string;
}) {
  const droppable = props.droppable;
  return (
    <div
      use:droppable
      classList={{
        [props.class]: true,
        'bg-blue-500/30': droppable.isActiveDroppable,
      }}
    />
  );
}

export const Pane: Component<{ paneId: string }> = (props) => {
  const dndContext = useDragDropContext();
  const isTabDragging = () => {
    if (!dndContext) return false;
    const [dndState] = dndContext;
    return typeof dndState.active.draggableId === 'string' &&
      dndState.active.draggableId.startsWith('tab:');
  };

  const tabGroupId = () => windowActions.findPaneById(props.paneId)?.tabGroupId ?? null;
  const group = () => (tabGroupId() ? windowStore.tabGroups[tabGroupId()!] : null);
  const tabs = () => group()?.tabs ?? [];
  const activeTab = () => {
    const g = group();
    if (!g?.activeTabId) return null;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? null;
  };
  const isActive = () => windowStore.activePaneId === props.paneId;

  const centerDroppable = createDroppable(`pane:${props.paneId}:center`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'center',
  });

  const leftDroppable = createDroppable(`pane:${props.paneId}:left`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'left',
  });
  const rightDroppable = createDroppable(`pane:${props.paneId}:right`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'right',
  });
  const topDroppable = createDroppable(`pane:${props.paneId}:top`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'top',
  });
  const bottomDroppable = createDroppable(`pane:${props.paneId}:bottom`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'bottom',
  });

  const handlePopOut = () => {
    const gid = tabGroupId();
    if (gid && tabs().length > 0) {
      windowActions.createFloatingWindow(gid, 150, 150, 500, 400);
    }
  };

  const renderContent = () => {
    const tab = activeTab();
    if (!tab) {
      return (
        <div class="flex-1 flex items-center justify-center bg-zinc-900/50">
          <div class="text-zinc-500 text-sm">No tab selected</div>
        </div>
      );
    }
    const panel = getGlobalRegistry().get(tab.contentType);
    if (panel) {
      return <Dynamic component={panel.component} />;
    }
    switch (tab.contentType) {
      case 'file':
        return (
          <div class="flex-1 bg-zinc-900 overflow-auto p-4">
            <pre class="text-sm font-mono text-zinc-300 whitespace-pre-wrap">
              {`// ${tab.title}\nimport { createRoot } from 'solid-js/web'\nimport App from './App'\n\nconst root = document.getElementById('root')\nif (root) createRoot(root).render(() => <App />)\n`}
            </pre>
          </div>
        );
      case 'document':
        return (
          <div class="flex-1 bg-zinc-900 overflow-auto p-6">
            <h1 class="text-2xl font-bold text-zinc-100 mb-4">Document Preview</h1>
            <div class="prose prose-invert max-w-none">
              <p class="text-zinc-300 leading-relaxed">
                This is a sample document. Drag tabs between panes, split panes, and pop out to
                floating windows.
              </p>
            </div>
          </div>
        );
      case 'preview':
        return (
          <div class="flex-1 bg-zinc-900 flex items-center justify-center p-8">
            <div class="bg-gradient-to-br from-zinc-800 to-zinc-900 rounded-xl p-8 border border-zinc-700">
              <div class="w-64 h-40 bg-zinc-700 rounded-lg flex items-center justify-center">
                <span class="text-zinc-500">Preview</span>
              </div>
            </div>
          </div>
        );
      case 'terminal':
        return (
          <div class="flex-1 bg-black font-mono text-sm p-3 overflow-auto">
            <div class="text-green-400 mb-2">$ bun run dev</div>
            <div class="text-zinc-300 mb-1">Starting dev server...</div>
            <div class="text-zinc-500 mb-2">
              <span class="text-green-400">$</span> <span class="animate-pulse">|</span>
            </div>
          </div>
        );
      case 'tool':
        return (
          <div class="flex-1 bg-zinc-900 overflow-auto p-4">
          </div>
        );
      default:
        return (
          <div class="flex-1 bg-zinc-900 flex items-center justify-center">
            <div class="text-zinc-500 text-sm">Unknown content type</div>
          </div>
        );
    }
  };

  return (
    <div
      use:centerDroppable
      classList={{
        'relative flex flex-col h-full overflow-hidden transition-all': true,
        'ring-1 ring-blue-500/30': isActive() && windowStore.focusedRegion === 'center',
        'bg-blue-500/5': centerDroppable.isActiveDroppable,
      }}
      onClick={() => windowActions.setActivePane(props.paneId)}
    >
      <Show
        when={tabs().length > 0}
        fallback={
          <div class="flex-1 flex flex-col items-center justify-center bg-zinc-900/30 border-2 border-dashed border-zinc-700 rounded text-zinc-500 text-sm gap-3">
            <AppWindow class="w-8 h-8 text-zinc-600" />
            <span>Drop tabs here</span>
          </div>
        }
      >
        <TabBar
          groupId={tabGroupId()!}
          paneId={props.paneId}
          onPopOut={handlePopOut}
        />
        {renderContent()}
      </Show>

      <Show when={centerDroppable.isActiveDroppable}>
        <div class="absolute inset-0 bg-blue-500/20 z-10 pointer-events-none" />
      </Show>

      <div
        classList={{
          'absolute inset-0 z-20': true,
          'pointer-events-auto': isTabDragging(),
          'pointer-events-none': !isTabDragging(),
        }}
      >
        <PaneDropZone
          position="top"
          droppable={topDroppable}
          class="absolute top-0 left-0 right-0 h-1/5 min-h-[24px]"
        />
        <PaneDropZone
          position="bottom"
          droppable={bottomDroppable}
          class="absolute bottom-0 left-0 right-0 h-1/5 min-h-[24px]"
        />
        <PaneDropZone
          position="left"
          droppable={leftDroppable}
          class="absolute top-0 bottom-0 left-0 w-1/5 min-w-[24px]"
        />
        <PaneDropZone
          position="right"
          droppable={rightDroppable}
          class="absolute top-0 bottom-0 right-0 w-1/5 min-w-[24px]"
        />
      </div>
    </div>
  );
};
