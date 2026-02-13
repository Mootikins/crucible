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
import { reorderState, setReorderState } from './TabBar';
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
import { matchShortcut } from '@/lib/keyboard-shortcuts';
import { smallestIntersecting } from '@/lib/collision-detector';

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
  const dndContext = useDragDropContext();
  const draggable = () => dndContext?.[0].active.draggable;
  const data = () => draggable()?.data as DragSource | undefined;

  return (
    <Show when={data()?.type === 'tab' || data()?.type === 'edgeTab'}>
      <div class="px-2.5 py-1.5 bg-zinc-800 border border-zinc-600 rounded shadow-lg text-xs text-zinc-200 flex items-center gap-1.5 opacity-90">
        <span class="font-medium truncate max-w-[120px]">
          {(() => { const d = data(); return (d?.type === 'tab' || d?.type === 'edgeTab') ? d.tab.title : ''; })()}
        </span>
      </div>
    </Show>
  );
}

function InnerManager() {
  const dndCtx = useDragDropContext()!;
  const [, { onDragEnd }] = dndCtx;

  onDragEnd(({ draggable, droppable }) => {
    const source = draggable.data as DragSource | undefined;
    const target = droppable?.data as DropTarget | undefined;

    const reorder = reorderState();
    setReorderState(null);
    if (reorder && source) {
      if (reorder.type === 'center' && source.type === 'tab' && reorder.groupId === source.sourceGroupId) {
        windowActions.moveTab(source.sourceGroupId, source.sourceGroupId, source.tab.id, reorder.insertIndex);
        return;
      }
      if (reorder.type === 'edge' && source.type === 'edgeTab' && reorder.position === source.sourcePosition) {
        windowActions.reorderEdgeTab(source.sourcePosition, source.tab.id, reorder.insertIndex);
        return;
      }
    }

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
        const targetPosition = target.panelId as 'left' | 'right' | 'bottom';
        const panel = windowStore.edgePanels[targetPosition];
        if (panel) {
          windowActions.moveCenterTabToEdge(source.sourceGroupId, source.tab.id, targetPosition);
          // Expand panel if collapsed
          if (panel.isCollapsed) {
            windowActions.setEdgePanelCollapsed(targetPosition, false);
          }
        }
      } else if (target.type === 'newFloating') {
        const newGroupId = windowActions.createTabGroup();
        windowActions.moveTab(source.sourceGroupId, newGroupId, source.tab.id);
        windowActions.createFloatingWindow(newGroupId, 100, 100, 400, 300);
      }
    } else if (source.type === 'edgeTab') {
      if (target.type === 'pane') {
        const paneId = target.paneId;
        const position = target.position;
        if (
          position &&
          position !== 'center' &&
          (position === 'left' || position === 'right' || position === 'top' || position === 'bottom')
        ) {
          // Edge tab → directional pane drop: promote to center first, then split
          const tempGroupId = windowActions.createTabGroup();
          windowActions.moveEdgeTabToCenter(source.sourcePosition, source.tab.id, tempGroupId);
          windowActions.splitPaneAndDrop(paneId, position, tempGroupId, source.tab.id);
        } else {
          // Edge tab → center pane drop
          const existingId = windowActions.getPaneTabGroupId(paneId);
          if (existingId) {
            windowActions.moveEdgeTabToCenter(source.sourcePosition, source.tab.id, existingId);
          } else {
            const newGroupId = windowActions.createTabGroup(paneId);
            windowActions.moveEdgeTabToCenter(source.sourcePosition, source.tab.id, newGroupId);
          }
        }
      } else if (target.type === 'tabGroup') {
        windowActions.moveEdgeTabToCenter(source.sourcePosition, source.tab.id, target.groupId);
      } else if (target.type === 'edgePanel') {
        const targetPosition = target.panelId as 'left' | 'right' | 'bottom';
        const panel = windowStore.edgePanels[targetPosition];
        if (panel) {
          // Same position → no-op
          if (source.sourcePosition === targetPosition) {
            return;
          }
          windowActions.moveEdgeTabToEdge(source.sourcePosition, source.tab.id, targetPosition);
          // Expand panel if collapsed
          if (panel.isCollapsed) {
            windowActions.setEdgePanelCollapsed(targetPosition, false);
          }
        }
      } else if (target.type === 'newFloating') {
        // Edge tab → new floating window
        const newGroupId = windowActions.createTabGroup();
        windowActions.moveEdgeTabToCenter(source.sourcePosition, source.tab.id, newGroupId);
        windowActions.createFloatingWindow(newGroupId, 100, 100, 400, 300);
      }
    }
  });

  const handleShortcutAction = (action: string) => {
    if (action === 'closeActiveTab') {
      const activePaneId = windowStore.activePaneId;
      if (!activePaneId) return;
      const pane = windowActions.findPaneById(activePaneId);
      if (!pane?.tabGroupId) return;
      const group = windowActions.getTabGroup(pane.tabGroupId);
      if (!group?.activeTabId) return;
      windowActions.removeTab(pane.tabGroupId, group.activeTabId);
    } else if (action === 'nextTab') {
      const activePaneId = windowStore.activePaneId;
      if (!activePaneId) return;
      const pane = windowActions.findPaneById(activePaneId);
      if (!pane?.tabGroupId) return;
      const group = windowActions.getTabGroup(pane.tabGroupId);
      if (!group || group.tabs.length === 0) return;
      const currentIndex = group.tabs.findIndex((t) => t.id === group.activeTabId);
      const nextIndex = (currentIndex + 1) % group.tabs.length;
      const nextTab = group.tabs[nextIndex];
      if (nextTab) {
        windowActions.setActiveTab(pane.tabGroupId, nextTab.id);
      }
    } else if (action === 'splitVertical') {
      const activePaneId = windowStore.activePaneId;
      if (activePaneId) {
        windowActions.splitPane(activePaneId, 'vertical');
      }
    } else if (action === 'toggleLeftPanel') {
      windowActions.toggleEdgePanel('left');
    }
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        windowActions.closeFlyout();
        return;
      }
      const action = matchShortcut(e);
      if (action) {
        e.preventDefault();
        handleShortcutAction(action);
      }
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
        <FlyoutPanel />
      </div>
      <StatusBar />
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
    <DragDropProvider collisionDetector={smallestIntersecting}>
      <DragDropSensors>
        <InnerManager />
      </DragDropSensors>
    </DragDropProvider>
  );
};
