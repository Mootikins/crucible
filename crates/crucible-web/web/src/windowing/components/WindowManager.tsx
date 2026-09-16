import { Show, For, onMount, onCleanup, type ParentComponent } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import {
  DragDropProvider,
  DragDropSensors,
  useDragDropContext,
  DragOverlay,
} from '@thisbeyond/solid-dnd';
import { CenterTiling } from './CenterTiling';
import { EdgeHost } from './EdgeHost';
import { FloatingWindow } from './FloatingWindow';
import { MinimizedBar } from './MinimizedBar';
import { windowStore, windowActions } from '@/windowing/store';
import { collectLeafGroupIds, primaryEdgeGroupId } from '@/windowing/model/tree';
import type { DragSource, DropTarget, EdgePanelPosition } from '@/windowing/model/types';
import { isEdgeCollapsed } from '@/windowing/model/types';
import { elideTabTitle, getPendingReorder, clearPendingReorder } from './TabBar';
import { LAYOUT_ACTIONS, matchShortcut } from '@/windowing/shortcuts';
import { policy } from '@/windowing/store';
import {
  WindowingProvider,
  useWindowing,
  type WindowingContextValue,
} from '@/windowing/components/context';
import { confirmTabClose } from '@/windowing/model/tab-guards';
import { placeNewTab, resolveNewTabTarget } from '@/windowing/components/tab-placement';
import { lastPointerPosition } from '@/windowing/model/collision-detector';
import { smallestIntersecting } from '@/windowing/model/collision-detector';

// The window manager draws no title bar. The app hangs its chrome on the
// slots: the rail head and tail, and the corner of the centre. Everything else
// belongs to content.

function DragOverlayContent() {
  const dndContext = useDragDropContext();
  const draggable = () => dndContext?.[0].active.draggable;
  const data = () => draggable()?.data as DragSource | undefined;

  // The drag data is a registration-time snapshot; read the live tab from the
  // store so a rename during the drag shows in the overlay.
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
      <div data-testid="drag-overlay" class="px-2.5 py-1.5 bg-surface-overlay border border-hairline-strong rounded shadow-lg text-xs text-shell-ink flex items-center gap-1.5 opacity-90">
        <span class="font-medium truncate max-w-(--cru-measure-tab)" title={title()}>
          {elideTabTitle(title())}
        </span>
      </div>
    </Show>
  );
}

/** The middle column of the rail row. Its key never changes, so it never moves. */
function CentreColumn() {
  const windowing = useWindowing();
  return (
    <div class="flex-1 flex flex-col overflow-hidden min-w-0">
      {/* relative: the corner slot floats at this area's bottom-right. */}
      <div class="relative flex-1 flex flex-col overflow-hidden min-h-0">
        <CenterTiling />
        {windowing.slots.corner?.()}
      </div>
    </div>
  );
}

/**
 * The rail row, left to right: a rail, the centre, the other rail.
 *
 * `side` is absent on the centre, which is how the row tells the two apart.
 */
type RowSlot = { key: string; side?: EdgePanelPosition };

const rowSlots = (): RowSlot[] => [
  { key: windowStore.edgePanels.left.id, side: 'left' },
  { key: 'centre' },
  { key: windowStore.edgePanels.right.id, side: 'right' },
];

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
        collectLeafGroupIds(
          windowStore.edgePanels[target.panelId as 'left' | 'right'].layout
        ).includes(source.sourceGroupId);
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
        const targetPosition = target.panelId as 'left' | 'right';
        const panel = windowStore.edgePanels[targetPosition];
        const edgeGroupId = panel
          ? primaryEdgeGroupId(windowStore, targetPosition)
          : null;
        if (edgeGroupId) {
          windowActions.moveTab(source.sourceGroupId, edgeGroupId, source.tab.id, target.insertIndex);
          // Expand panel if collapsed
          if (isEdgeCollapsed(panel)) {
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

  /** The chords the layout owns. `LAYOUT_ACTIONS` names each of them. */
  const handleLayoutAction = (action: string) => {
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
    } else if (action === 'toggleRightPanel') {
      windowActions.toggleEdgePanel('right');
    } else if (action === 'swapSidePanels') {
      windowActions.swapSidePanels();
    }
  };

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const action = matchShortcut(e, policy().shortcuts);
      if (!action) return;
      if (LAYOUT_ACTIONS.has(action)) {
        e.preventDefault();
        handleLayoutAction(action);
        return;
      }
      if (policy().onShortcut(action, e)) e.preventDefault();
    };
    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  const floatingWindows = () =>
    windowStore.floatingWindows.filter((w) => !w.isMinimized);

  return (
    <div class="flex flex-col h-screen bg-shell-bg text-shell-ink overflow-hidden select-none">
      <div class="relative z-0 flex flex-1 overflow-hidden min-h-0">
        {/* The row is a KEYED LIST, not three fixed slots, so a flip MOVES a
            rail across the centre instead of rebuilding it. The two rails
            used to sit in fixed slots; `swapSidePanels` then handed each slot
            the other rail's tree, and Solid rebuilt both — every panel in
            them lost its state and loaded its content again from empty.

            The key is the panel's own `id`, which travels with its contents.
            So a flip reverses the two rail keys, `Key` moves the existing DOM
            node, and the surviving EdgeHost sees only its `position` prop
            change. The centre keeps a constant key and never moves. */}
        <Key each={rowSlots()} by="key">
          {(slot) => (
            <Show when={slot().side} fallback={<CentreColumn />}>
              {(side) => <EdgeHost position={side()} />}
            </Show>
          )}
        </Key>
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

/**
 * The window manager. The app gives it the tab renderer and the chrome slots;
 * `children` render inside the drag provider, so an app overlay there can
 * start a tab drag.
 */
export const WindowManager: ParentComponent<WindowingContextValue> = (props) => {
  return (
    <WindowingProvider renderContent={props.renderContent} slots={props.slots}>
      <DragDropProvider collisionDetector={smallestIntersecting}>
        <DragDropSensors>
          <InnerManager />
          {props.children}
        </DragDropSensors>
      </DragDropProvider>
    </WindowingProvider>
  );
};
