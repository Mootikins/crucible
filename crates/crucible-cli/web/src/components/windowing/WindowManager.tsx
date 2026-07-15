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
import { getPendingReorder, clearPendingReorder } from './TabBar';
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
import { confirmTabClose } from '@/lib/tab-guards';
import { openPanelTab } from '@/lib/panel-actions';
import { smallestIntersecting } from '@/lib/collision-detector';
import { statusBarStore, statusBarActions, pathBasename } from '@/stores/statusBarStore';
import { shellStore, shellActions } from '@/stores/shellStore';
import { attentionStore } from '@/stores/attentionStore';

/** One expandable pill in the Edit ↔ Session mode toggle: icon-only until
 * hovered, ember-filled when its surface is active. */
function ModePill(props: {
  glyph: string;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={props.label}
      onClick={props.onClick}
      class="group flex items-center gap-1.5 box-border h-6 rounded-full px-[7px] cursor-pointer max-w-[27px] hover:max-w-[110px] overflow-hidden transition-all duration-300"
      classList={{
        'bg-primary text-white': props.active,
        'text-muted hover:text-shell-ink': !props.active,
      }}
    >
      <span class="text-xs flex-none">{props.glyph}</span>
      <span class="text-[11px] font-semibold whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity duration-300">
        {props.label}
      </span>
    </button>
  );
}

function HeaderBar() {
  const edgePanels = () => windowStore.edgePanels;
  const surface = shellStore.activeSurface;
  const badge = attentionStore.attentionCount;
  const kilnName = () => pathBasename(statusBarStore.kilnPath());

  const contextLine = () => {
    switch (surface()) {
      case 'home':
        return 'pick up where you left off';
      case 'edit':
        return kilnName() ? `editing ◆ ${kilnName()}` : 'editing';
      case 'session':
        return statusBarStore.activeSessionTitle() ?? 'session';
      case 'inbox':
        return 'everything waiting on you, one place';
    }
  };

  return (
    <div class="flex items-center h-10 gap-3 bg-shell-bg border-b border-white/[0.07] px-3.5">
      <button
        type="button"
        title="Home"
        onClick={() => shellActions.goHome()}
        class="w-[18px] h-[18px] rounded-[5px] bg-primary flex items-center justify-center font-mono font-semibold text-[10px] text-white cursor-pointer hover:ring-[3px] hover:ring-primary/30 transition-shadow"
      >
        C
      </button>
      <Show when={kilnName()}>
        <button
          type="button"
          title="Home"
          onClick={() => shellActions.goHome()}
          class="font-mono text-xs text-muted hover:text-shell-ink cursor-pointer"
        >
          ◆ {kilnName()}
        </button>
      </Show>
      <div class="flex bg-shell-panel border border-white/10 rounded-full p-0.5 gap-0.5">
        <ModePill
          glyph="✎"
          label="Edit"
          active={surface() === 'edit'}
          onClick={() => shellActions.goEdit()}
        />
        <ModePill
          glyph="◆"
          label="Session"
          active={surface() === 'session'}
          onClick={() => shellActions.goSession()}
        />
      </div>
      <span class="font-mono text-[10.5px] text-muted-dark truncate max-w-[320px]">
        {contextLine()}
      </span>
      <span class="flex-1" />
      <button
        type="button"
        title="Inbox"
        onClick={() => shellActions.goInbox()}
        class="relative flex items-center gap-1.5 px-2.5 py-1 rounded-md cursor-pointer text-xs border transition-colors hover:bg-surface-elevated"
        classList={{
          'text-primary border-primary/50': surface() === 'inbox',
          'text-muted border-white/[0.08]': surface() !== 'inbox',
        }}
      >
        ▤ Inbox
        <Show when={badge() > 0}>
          <span class="min-w-[15px] h-[15px] rounded-full bg-attention text-black font-mono font-bold text-[9.5px] flex items-center justify-center px-[3px]">
            {badge()}
          </span>
        </Show>
      </button>
      <button
        type="button"
        title="Command palette (Ctrl+P)"
        class="flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-white/[0.08] text-xs text-muted cursor-pointer hover:bg-surface-elevated hover:text-shell-ink transition-colors"
        onClick={() => window.dispatchEvent(new CustomEvent('crucible:open-command-palette'))}
      >
        <IconZap class="w-3 h-3" />
        <kbd class="font-mono text-[10px]">Ctrl+P</kbd>
      </button>
      <div class="flex items-center gap-0.5">
        <button
          type="button"
          class="p-1.5 text-muted-dark hover:text-shell-ink hover:bg-surface-elevated rounded transition-colors"
          title={edgePanels().left.isCollapsed ? 'Show Left Panel' : 'Hide Left Panel'}
          onClick={() => windowActions.toggleEdgePanel('left')}
        >
          {edgePanels().left.isCollapsed ? (
            <IconPanelLeft class="w-4 h-4" />
          ) : (
            <IconPanelLeftClose class="w-4 h-4" />
          )}
        </button>
        <button
          type="button"
          class="p-1.5 text-muted-dark hover:text-shell-ink hover:bg-surface-elevated rounded transition-colors"
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
          class="p-1.5 text-muted-dark hover:text-shell-ink hover:bg-surface-elevated rounded transition-colors"
          title={edgePanels().right.isCollapsed ? 'Show Right Panel' : 'Hide Right Panel'}
          onClick={() => windowActions.toggleEdgePanel('right')}
        >
          {edgePanels().right.isCollapsed ? (
            <IconPanelRight class="w-4 h-4" />
          ) : (
            <IconPanelRightClose class="w-4 h-4" />
          )}
        </button>
        <div class="w-px h-4 bg-white/10 mx-1" />
        <button
          type="button"
          class="p-1.5 text-muted-dark hover:text-shell-ink hover:bg-surface-elevated rounded transition-colors"
          title="Open Settings"
          onClick={() => openPanelTab('settings')}
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

  // The drag data is a registration-time snapshot; read the live tab from the
  // store so a mid-session rename (daemon title push) shows in the overlay.
  const title = () => {
    const d = data();
    if (d?.type !== 'tab') return '';
    const live = windowStore.tabGroups[d.sourceGroupId]?.tabs.find((t) => t.id === d.tab.id);
    return (live ?? d.tab).title;
  };

  return (
    <Show when={data()?.type === 'tab'}>
      <div class="px-2.5 py-1.5 bg-zinc-800 border border-zinc-600 rounded shadow-lg text-xs text-zinc-200 flex items-center gap-1.5 opacity-90">
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
    } else if (action === 'closeOverlay') {
      windowActions.closeFlyout();
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
    <div class="flex flex-col h-screen bg-shell-bg text-shell-ink overflow-hidden select-none">
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
