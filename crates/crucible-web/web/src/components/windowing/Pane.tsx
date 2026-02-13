import { Component, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { createDroppable, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from './TabBar';
import { windowStore, windowActions } from '@/stores/windowStore';
import { getGlobalRegistry } from '@/lib/panel-registry';

type PaneDropPosition = 'left' | 'right' | 'top' | 'bottom';

function PaneDropZone(props: {
  position: PaneDropPosition;
  droppable: ReturnType<typeof createDroppable>;
  class: string;
}) {
  const droppable = props.droppable;
  const labels: Record<PaneDropPosition, string> = {
    left: 'Split Left',
    right: 'Split Right',
    top: 'Split Top',
    bottom: 'Split Bottom',
  };
  return (
    <div
      use:droppable
      classList={{
        [props.class]: true,
        'bg-blue-500/20 border-2 border-blue-400 border-dashed': droppable.isActiveDroppable,
        'hover:bg-zinc-700/20 transition-all duration-150': !droppable.isActiveDroppable,
      }}
    >
      {droppable.isActiveDroppable && (
        <div class="absolute inset-0 flex items-center justify-center">
          <span class="text-xs font-medium text-blue-300 bg-blue-500/30 px-2 py-1 rounded">
            {labels[props.position]}
          </span>
        </div>
      )}
    </div>
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
            <div class="flex items-center gap-2 mb-4 pb-3 border-b border-zinc-800">
              <span class="text-sm text-zinc-300">{tab.title}</span>
              {tab.isModified && (
                <span class="text-xs text-amber-500 px-1.5 py-0.5 bg-amber-500/10 rounded">
                  Modified
                </span>
              )}
            </div>
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
              <p class="text-sm text-zinc-400 mt-3 text-center">{tab.title}</p>
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
            <div class="text-zinc-300 text-sm">Tool: {tab.title}</div>
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
        'ring-1 ring-blue-500/30': isActive(),
        'bg-blue-500/5': centerDroppable.isActiveDroppable,
      }}
      onClick={() => windowActions.setActivePane(props.paneId)}
    >
      <Show
        when={tabs().length > 0}
        fallback={
          <div class="flex-1 flex items-center justify-center bg-zinc-900/30 border-2 border-dashed border-zinc-700 rounded text-zinc-500 text-sm">
            Drop tabs here
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

      {/* Merge overlay when center drop zone is active */}
      <Show when={centerDroppable.isActiveDroppable}>
        <div class="absolute inset-0 bg-blue-500/10 flex items-center justify-center z-10 pointer-events-none border-2 border-blue-400 border-dashed rounded">
          <span class="text-sm font-medium text-blue-300 bg-blue-500/30 px-3 py-1.5 rounded-lg">
            Merge
          </span>
        </div>
      </Show>

      {/* Drop zone overlays when dragging a tab: split left/right/top/bottom */}
      <Show when={isTabDragging()}>
        <div class="absolute inset-0 pointer-events-none">
          <PaneDropZone
            position="top"
            droppable={topDroppable}
            class="absolute top-0 left-0 right-0 h-1/5 min-h-[24px] pointer-events-auto"
          />
          <PaneDropZone
            position="bottom"
            droppable={bottomDroppable}
            class="absolute bottom-0 left-0 right-0 h-1/5 min-h-[24px] pointer-events-auto"
          />
          <PaneDropZone
            position="left"
            droppable={leftDroppable}
            class="absolute top-1/5 bottom-1/5 left-0 w-1/5 min-w-[24px] pointer-events-auto"
          />
          <PaneDropZone
            position="right"
            droppable={rightDroppable}
            class="absolute top-1/5 bottom-1/5 right-0 w-1/5 min-w-[24px] pointer-events-auto"
          />
        </div>
      </Show>
    </div>
  );
};
