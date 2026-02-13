import { Component, Show, For, onMount, onCleanup } from 'solid-js';
import {
  DragDropProvider,
  DragDropSensors,
  useDragDropContext,
  DragOverlay,
} from '@thisbeyond/solid-dnd';
import { CenterTiling } from './CenterTiling';
import { EdgePanel } from './EdgePanel';
import { FlyoutPanel } from './FlyoutPanel';
import { FloatingWindow } from './FloatingWindow';
import { StatusBar } from './StatusBar';
import { MinimizedBar } from './MinimizedBar';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { DragSource, DropTarget } from '@/types/windowTypes';
import {
  IconPanelLeft,
  IconPanelLeftClose,
  IconPanelRight,
  IconPanelRightClose,
  IconPanelBottom,
  IconPanelBottomClose,
  IconSettings,
  IconZap,
} from './icons';

function HeaderBar() {
  const edgePanels = () => windowStore.edgePanels;

  return (
    <div class="flex items-center h-8 bg-zinc-900 border-b border-zinc-800 px-2">
      <button
        type="button"
        class="p-1.5 text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors"
        title={edgePanels().left.isCollapsed ? 'Show Left Panel' : 'Hide Left Panel'}
        onClick={() => windowActions.toggleEdgePanel('left')}
      >
        {edgePanels().left.isCollapsed ? (
          <IconPanelLeft class="w-4 h-4" />
        ) : (
          <IconPanelLeftClose class="w-4 h-4" />
        )}
      </button>
      <div class="flex-1 flex justify-center">
        <div class="flex items-center gap-1.5 px-3 py-1 bg-zinc-800/50 rounded border border-zinc-700/50 text-xs text-zinc-400 cursor-pointer hover:bg-zinc-800 transition-colors">
          <IconZap class="w-3 h-3" />
          <span>Command palette</span>
          <kbd class="ml-1 px-1 py-0.5 bg-zinc-700 rounded text-[10px] text-zinc-500">
            ⌘P
          </kbd>
        </div>
      </div>
      <div class="flex items-center gap-0.5">
        <button
          type="button"
          class="p-1.5 text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors"
          title={edgePanels().bottom.isCollapsed ? 'Show Bottom Panel' : 'Hide Bottom Panel'}
          onClick={() => windowActions.toggleEdgePanel('bottom')}
        >
          {edgePanels().bottom.isCollapsed ? (
            <IconPanelBottom class="w-4 h-4" />
          ) : (
            <IconPanelBottomClose class="w-4 h-4" />
          )}
        </button>
        <button
          type="button"
          class="p-1.5 text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors"
          title={edgePanels().right.isCollapsed ? 'Show Right Panel' : 'Hide Right Panel'}
          onClick={() => windowActions.toggleEdgePanel('right')}
        >
          {edgePanels().right.isCollapsed ? (
            <IconPanelRight class="w-4 h-4" />
          ) : (
            <IconPanelRightClose class="w-4 h-4" />
          )}
        </button>
        <div class="w-px h-4 bg-zinc-700 mx-1" />
        <button
          type="button"
          class="p-1.5 text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors"
        >
          <IconSettings class="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}

function DragOverlayContent() {
  const [state] = useDragDropContext();
  const draggable = () => state.active.draggable;
  const data = () => draggable()?.data as DragSource | undefined;

  return (
    <Show when={data()?.type === 'tab'}>
      <div class="px-2.5 py-1.5 bg-zinc-800 border border-zinc-600 rounded shadow-lg text-xs text-zinc-200 flex items-center gap-1.5 opacity-90">
        <span class="font-medium truncate max-w-[120px]">
          {data()?.type === 'tab' ? data()!.tab.title : ''}
        </span>
      </div>
    </Show>
  );
}

function InnerManager() {
  const [, { onDragEnd }] = useDragDropContext();

  onDragEnd(({ draggable, droppable }) => {
    const source = draggable.data as DragSource | undefined;
    const target = droppable?.data as DropTarget | undefined;
    if (!source || !target) {
      if (source?.type === 'tab' && draggable.id === 'newFloating') {
        // Dropped on "New Window" chip or similar - create floating
        const newGroupId = windowActions.createTabGroup();
        windowActions.moveTab(source.sourceGroupId, newGroupId, source.tab.id);
        windowActions.createFloatingWindow(newGroupId, 100, 100, 400, 300);
      }
      return;
    }
    if (source.type === 'tab') {
      if (target.type === 'pane') {
        const paneId = target.paneId;
        const position = target.position;
        if (
          position &&
          position !== 'center' &&
          (position === 'left' || position === 'right' || position === 'top' || position === 'bottom')
        ) {
          windowActions.splitPaneAndDrop(
            paneId,
            position,
            source.sourceGroupId,
            source.tab.id
          );
        } else {
          const existingId = windowActions.getPaneTabGroupId(paneId);
          if (existingId) {
            windowActions.moveTab(source.sourceGroupId, existingId, source.tab.id);
          } else {
            const newGroupId = windowActions.createTabGroup(paneId);
            windowActions.moveTab(source.sourceGroupId, newGroupId, source.tab.id);
          }
        }
      } else if (target.type === 'tabGroup') {
        windowActions.moveTab(
          source.sourceGroupId,
          target.groupId,
          source.tab.id,
          target.insertIndex
        );
      } else if (target.type === 'edgePanel') {
        const panel = windowStore.edgePanels[target.panelId as 'left' | 'right' | 'bottom'];
        if (panel) {
          windowActions.removeTab(source.sourceGroupId, source.tab.id);
          windowActions.addEdgePanelTab(panel.position, {
            ...source.tab,
            panelPosition: panel.position,
          });
        }
      } else if (target.type === 'newFloating') {
        const newGroupId = windowActions.createTabGroup();
        windowActions.moveTab(source.sourceGroupId, newGroupId, source.tab.id);
        windowActions.createFloatingWindow(newGroupId, 100, 100, 400, 300);
      }
    }
  });

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') windowActions.closeFlyout();
    };
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  const floatingWindows = () =>
    windowStore.floatingWindows.filter((w) => !w.isMinimized);

  return (
    <div class="flex flex-col h-screen bg-zinc-950 text-zinc-100 overflow-hidden select-none">
      <HeaderBar />
      <div class="relative z-0 flex flex-1 overflow-hidden min-h-0">
        <EdgePanel position="left" />
        <div class="flex-1 flex flex-col overflow-hidden min-w-0">
          <CenterTiling />
          <EdgePanel position="bottom" />
        </div>
        <EdgePanel position="right" />
      </div>
      <StatusBar />
      <FlyoutPanel />
      <div class="fixed inset-0 z-30 pointer-events-none">
        <For each={floatingWindows()}>
          {(w) => (
            <div class="pointer-events-auto">
              <FloatingWindow window={w} />
            </div>
          )}
        </For>
      </div>
      <MinimizedBar />
      <DragOverlay>
        <DragOverlayContent />
      </DragOverlay>
    </div>
  );
}

export const WindowManager: Component = () => {
  return (
    <DragDropProvider>
      <DragDropSensors>
        <InnerManager />
      </DragDropSensors>
    </DragDropProvider>
  );
};
