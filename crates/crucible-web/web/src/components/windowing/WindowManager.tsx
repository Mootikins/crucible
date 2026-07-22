import { Component, Show, For, onMount, onCleanup } from 'solid-js';
import {
  DragDropProvider,
  DragDropSensors,
  useDragDropContext,
  DragOverlay,
} from '@thisbeyond/solid-dnd';
import { CenterTiling } from './CenterTiling';
import { EdgePanel } from './EdgePanel';
import { FloatingWindow } from './FloatingWindow';
import { CornerBar } from './CornerBar';
import { MinimizedBar } from './MinimizedBar';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { DragSource, DropTarget } from '@/types/windowTypes';
import { getPendingReorder, clearPendingReorder } from './TabBar';
import { matchShortcut } from '@/lib/keyboard-shortcuts';
import { confirmTabClose } from '@/lib/tab-guards';
import { placeNewTab, resolveNewTabTarget } from '@/lib/tab-placement';
import { lastPointerPosition } from '@/lib/collision-detector';
import { WikilinkHoverPreview } from '@/components/WikilinkHoverPreview';
import { smallestIntersecting } from '@/lib/collision-detector';
import { statusBarStore, statusBarActions } from '@/stores/statusBarStore';

// There is no header bar: the edge ribbons carry the shell chrome (panel
// toggles, palette, new session, settings) and the status bar carries the
// context + attention indicators. The center belongs entirely to content.

function DragOverlayContent() {
  const dndContext = useDragDropContext();
  const draggable = () => dndContext?.[0].active.draggable;
  const data = () => draggable()?.data as DragSource | undefined;

  // The drag data is a registration-time snapshot; read the live tab from the
  // store so a mid-session rename (daemon title push) shows in the overlay.
  // 'newTab' sources have no group yet — their snapshot IS the live tab.
  const title = () => {
    const d = data();
    if (d?.type === 'newTab') return d.tab.title;
    if (d?.type !== 'tab') return '';
    const live = windowStore.tabGroups[d.sourceGroupId]?.tabs.find((t) => t.id === d.tab.id);
    return (live ?? d.tab).title;
  };

  return (
    <Show when={data()?.type === 'tab' || data()?.type === 'newTab'}>
      <div class="px-2.5 py-1.5 bg-surface-overlay border border-hairline-strong rounded shadow-lg text-xs text-shell-ink flex items-center gap-1.5 opacity-90">
        <span class="font-medium truncate max-w-[120px]">{title()}</span>
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

    const reorder = getPendingReorder();
    clearPendingReorder();
    if (reorder && source && source.type === 'tab' && reorder.groupId === source.sourceGroupId) {
      const droppingOnSameGroup =
        target?.type === 'tabGroup' && target.groupId === source.sourceGroupId;
      const droppingOnSameEdgePanel =
        target?.type === 'edgePanel' &&
        windowStore.edgePanels[target.panelId as 'left' | 'right' | 'bottom']?.tabGroupId ===
          source.sourceGroupId;
      if (!target || droppingOnSameGroup || droppingOnSameEdgePanel) {
        windowActions.moveTab(source.sourceGroupId, source.sourceGroupId, source.tab.id, reorder.insertIndex);
        return;
      }
    }

    if (source?.type === 'newTab') {
      // Hover-editor semantics: dock on explicit targets, otherwise tear
      // off into a floating window at the release point.
      placeNewTab(resolveNewTabTarget(target, lastPointerPosition()), source.tab);
      return;
    }
    if (!source || !target) return;
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
          const edgeGroupId = panel.tabGroupId;
          windowActions.moveTab(source.sourceGroupId, edgeGroupId, source.tab.id, target.insertIndex);
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
      const activeTab = group.tabs.find((t) => t.id === group.activeTabId);
      if (activeTab && !confirmTabClose(activeTab)) return;
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
    } else if (action === 'openCommandPalette') {
      // Handled by App.tsx in capture phase
    } else if (action === 'focusChatInput') {
      const el = document.querySelector<HTMLTextAreaElement>('textarea[data-testid="chat-input"]');
      el?.focus();
    } else if (action === 'newSession') {
      window.dispatchEvent(new CustomEvent('crucible:new-session'));
    } else if (action === 'toggleRightPanel') {
      windowActions.toggleEdgePanel('right');
    } else if (action === 'toggleBottomPanel') {
      windowActions.toggleEdgePanel('bottom');
    } else if (action === 'clearChat') {
      window.dispatchEvent(new CustomEvent('crucible:clear-chat'));
    } else if (action === 'toggleThinking') {
      const current = statusBarStore.showThinking();
      statusBarActions.setShowThinking(!current);
    }
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
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
    <div class="flex flex-col h-screen bg-shell-bg text-shell-ink overflow-hidden select-none">
      <div class="relative z-0 flex flex-1 overflow-hidden min-h-0">
        <EdgePanel position="left" />
        <div class="flex-1 flex flex-col overflow-hidden min-w-0">
          {/* relative: CornerBar floats at this area's bottom-right (above
              the bottom dock), Adobe-style — the status bar's replacement. */}
          <div class="relative flex-1 flex flex-col overflow-hidden min-h-0">
            <CenterTiling />
            <CornerBar />
          </div>
          <EdgePanel position="bottom" />
        </div>
        <EdgePanel position="right" />
      </div>
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
        {/* Inside the provider so hover cards can drag file tabs into the
            window system (DragSource 'newTab'). */}
        <WikilinkHoverPreview />
      </DragDropSensors>
    </DragDropProvider>
  );
};
