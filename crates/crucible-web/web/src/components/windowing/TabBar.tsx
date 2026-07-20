import { Component, For, JSX, Show, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import {
  createDraggable,
  createDroppable,
  useDragDropContext,
} from '@thisbeyond/solid-dnd';
import type { Tab as TabType, EdgePanelPosition, TabBarProps, DragSource } from '@/types/windowTypes';
import { windowStore, windowActions } from '@/stores/windowStore';
import { IconGripVertical, IconClose, IconLayout } from './icons';
import { ChevronDown } from '@/lib/icons';
import { confirmTabClose } from '@/lib/tab-guards';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { attachNativeMenuGuard, tabsToClose, type TabCloseMode } from '@/lib/context-menu';

// ── Module-level reorder state (shared with WindowManager) ──────────────

export type ReorderState = {
  groupId: string;
  insertIndex: number;
} | null;

const [reorderState, setReorderState] = createSignal<ReorderState>(null);
export { reorderState, setReorderState };

// Non-reactive pending reorder state (survives reactive cleanup race)
let pendingReorder: ReorderState = null;

export function getPendingReorder(): ReorderState {
  return pendingReorder;
}

export function clearPendingReorder(): void {
  pendingReorder = null;
}

// ── Insert-index computation helper ─────────────────────────────────────

function computeInsertIndex(
  containerEl: HTMLElement,
  pointerX: number,
  draggedTabId?: string,
): { logical: number; display: number } | null {
  const tabEls = containerEl.querySelectorAll('[data-tab-id]');
  let logicalIndex = 0;
  for (let i = 0; i < tabEls.length; i++) {
    const el = tabEls[i] as HTMLElement;
    if (draggedTabId && el.dataset.tabId === draggedTabId) continue;
    const rect = el.getBoundingClientRect();
    if (pointerX < rect.left + rect.width / 2) return { logical: logicalIndex, display: i };
    logicalIndex++;
  }
  return { logical: logicalIndex, display: tabEls.length };
}

// ── Unified TabItem (replaces Tab + EdgeTab) ────────────────────────────

interface TabItemProps {
  tab: TabType;
  draggableId: string;
  draggableData: DragSource;
  isActive: boolean;
  isFocused: boolean;
  onClick: () => void;
  onClose: (e: MouseEvent) => void;
  testId?: string;
}

const TabItem: Component<TabItemProps> = (props) => {
  const draggable = createDraggable(props.draggableId, props.draggableData);
  const Icon = props.tab.icon;

  return (
    <div
      use:draggable
      data-tab-id={props.tab.id}
      {...(props.testId ? { 'data-testid': props.testId } : {})}
      classList={{
        'group relative flex items-center gap-1 px-2.5 py-1.5 cursor-pointer transition-all duration-100 border-b-2 rounded-t-sm':
          true,
        'opacity-40 border-transparent bg-surface-elevated': draggable.isActiveDraggable,
        'bg-shell-bg text-shell-ink': props.isActive && !draggable.isActiveDraggable,
        'border-primary': props.isActive && props.isFocused && !draggable.isActiveDraggable,
        'border-hairline': props.isActive && !props.isFocused && !draggable.isActiveDraggable,
        'border-transparent text-muted hover:text-shell-ink hover:bg-hover-wash':
          !props.isActive && !draggable.isActiveDraggable,
      }}
      onClick={() => props.onClick()}
    >
      <div class="relative w-3.5 h-3.5 flex-shrink-0 cursor-grab active:cursor-grabbing">
        <Show when={Icon} fallback={
          <IconGripVertical class="w-3.5 h-3.5 text-muted-dark opacity-0 group-hover:opacity-100 transition-opacity duration-150" />
        }>
          <>
            {Icon && <Icon class={`absolute inset-0 w-3.5 h-3.5 ${props.isActive ? 'text-shell-body' : 'text-muted-dark'} group-hover:opacity-0 transition-opacity duration-150`} />}
            <IconGripVertical class="absolute inset-0 w-3.5 h-3.5 text-muted-dark opacity-0 group-hover:opacity-100 transition-opacity duration-150" />
          </>
        </Show>
      </div>
      <span class="text-xs font-medium truncate max-w-[120px]">
        {props.tab.title}
      </span>
      {props.tab.isModified && (
        <span class="w-1.5 h-1.5 rounded-full bg-attention flex-shrink-0" />
      )}
      <button
        aria-label="Close tab"
        onClick={(e) => {
          e.stopPropagation();
          props.onClose(e);
        }}
        classList={{
          'flex-shrink-0 p-0.5 rounded-sm transition-all hover:bg-hover-wash hover:text-shell-ink focus:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary': true,
          'opacity-0 group-hover:opacity-100': !props.isActive,
        }}
      >
        <IconClose class="w-3 h-3" />
      </button>
    </div>
  );
};

// ── Insert indicator element ────────────────────────────────────────────

const InsertIndicator: Component = () => (
  <div class="w-0.5 h-5 bg-primary rounded-full flex-shrink-0 my-auto" />
);

interface UseTabBarDnDOptions {
  groupId: () => string;
  tabsContainerRef: () => HTMLDivElement | undefined;
}

function useTabBarDnD(options: UseTabBarDnDOptions) {
  const [insertIdx, setInsertIdx] = createSignal<number | null>(null);
  const dndCtx = useDragDropContext();

  const isSameBarDrag = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return false;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' && data.sourceGroupId === options.groupId();
  };

  const draggedTabId = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return undefined;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' ? data.tab.id : undefined;
  };

  createEffect(() => {
    const tabsContainerRef = options.tabsContainerRef();
    if (!isSameBarDrag() || !tabsContainerRef) {
      setInsertIdx(null);
      setReorderState(null);
      return;
    }
    const sensor = dndCtx?.[0]?.active?.sensor;
    const x = sensor?.coordinates?.current?.x;
    const y = sensor?.coordinates?.current?.y;
    if (x != null && y != null) {
      const rect = tabsContainerRef.getBoundingClientRect();
      const VERTICAL_TOLERANCE = 8;
      const inBounds = x >= rect.left && x <= rect.right &&
                       y >= rect.top - VERTICAL_TOLERANCE && y <= rect.bottom + VERTICAL_TOLERANCE;
      if (!inBounds) {
        setInsertIdx(null);
        setReorderState(null);
        // Also drop the non-reactive copy: leaving it set would apply a stale
        // reorder when the tab is released outside the bar. Safe to clear here
        // (unlike the no-active-draggable path) because this branch only runs
        // mid-drag for this bar's own tab.
        pendingReorder = null;
        return;
      }
      const result = computeInsertIndex(tabsContainerRef, x, draggedTabId());
      setInsertIdx(result?.display ?? null);
      if (result != null) {
        const nextReorder = { groupId: options.groupId(), insertIndex: result.logical };
        setReorderState(nextReorder);
        pendingReorder = nextReorder;
      }
    } else {
      setInsertIdx(null);
      setReorderState(null);
      pendingReorder = null;
    }
  });

  createEffect(() => {
    if (!dndCtx?.[0]?.active?.draggable) {
      setInsertIdx(null);
      setReorderState(null);
    }
  });

  return {
    insertIdx,
  };
}

interface TabStripProps {
  tabs: () => TabType[];
  activeTabId: () => string | null;
  insertIdx: () => number | null;
  onSelectTab: (tabId: string) => void;
  onTabsContainerRef?: (el: HTMLDivElement) => void;
  renderTab: (tab: () => TabType, index: () => number) => JSX.Element;
}

const TabStrip: Component<TabStripProps> = (props) => {
  const [isOverflowing, setIsOverflowing] = createSignal(false);
  const [showDropdown, setShowDropdown] = createSignal(false);
  let tabsContainerRef: HTMLDivElement | undefined;

  onMount(() => {
    if (!tabsContainerRef) return;
    const checkOverflow = () => {
      if (tabsContainerRef) {
        setIsOverflowing(tabsContainerRef.scrollWidth > tabsContainerRef.clientWidth);
      }
    };
    const observer = new ResizeObserver(checkOverflow);
    observer.observe(tabsContainerRef);
    createEffect(() => {
      props.tabs();
      checkOverflow();
    });
    onCleanup(() => observer.disconnect());
  });

  createEffect(() => {
    if (!showDropdown()) return;
    const handleClickOutside = () => {
      setShowDropdown(false);
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShowDropdown(false);
    };
    setTimeout(() => {
      document.addEventListener('click', handleClickOutside);
      document.addEventListener('keydown', handleEscape);
    }, 0);
    onCleanup(() => {
      document.removeEventListener('click', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    });
  });

  return (
    <>
      <div
        ref={(el) => {
          tabsContainerRef = el;
          props.onTabsContainerRef?.(el);
        }}
        class="flex-1 flex items-end gap-0.5 overflow-x-auto scrollbar-hide px-1 min-w-0 [scrollbar-width:none] [-ms-overflow-style:none]"
      >
        {/* Keyed by tab id, NOT object identity: updateTab replaces the tab
            object on every write (dirty flag, title), and a remounting row
            re-registers its solid-dnd draggable under the same id — the old
            row's cleanup then deletes the NEW registration, leaving the tab
            silently undraggable ("Cannot remove nonexistent draggable" at
            unmount). Key keeps the row alive across object replacement. */}
        <Key each={props.tabs()} by={(t) => t.id}>
          {(tab, i) => (
            <>
              <Show when={props.insertIdx() === i()}>
                <InsertIndicator />
              </Show>
              {props.renderTab(tab, i)}
            </>
          )}
        </Key>
        <Show when={props.insertIdx() === props.tabs().length}>
          <InsertIndicator />
        </Show>
      </div>
      <Show when={isOverflowing()}>
        <div class="relative flex-shrink-0">
          <button
            class="flex-shrink-0 w-6 h-6 flex items-center justify-center text-muted-dark hover:text-shell-body hover:bg-hover-wash rounded transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
            aria-label="Show all tabs"
            onClick={(e) => { e.stopPropagation(); setShowDropdown(!showDropdown()); }}
            title="Show all tabs"
          >
            <ChevronDown class="w-3.5 h-3.5" />
          </button>
          <Show when={showDropdown()}>
            <div class="absolute right-0 top-full mt-1 z-50 min-w-[160px] max-w-[280px] bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 max-h-[300px] overflow-y-auto">
              <For each={props.tabs()}>
                {(tab) => (
                  <button
                    class={`w-full px-3 py-1.5 text-left text-xs truncate transition-colors ${
                      tab.id === props.activeTabId()
                        ? 'bg-primary/20 text-primary font-medium'
                        : 'text-shell-body hover:bg-hover-wash'
                    }`}
                    onClick={() => {
                      props.onSelectTab(tab.id);
                      setShowDropdown(false);
                      const tabEl = tabsContainerRef?.querySelector(`[data-tab-id="${tab.id}"]`);
                      tabEl?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
                    }}
                  >
                    {tab.title}
                    {tab.isModified && <span class="ml-1 text-attention">●</span>}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
    </>
  );
};

// ── Tab context menu ────────────────────────────────────────────────────

/**
 * Right-click menu on a tab: Close / Close Others / Close to the Right —
 * the classic victims of browser-owned keybinds (Ctrl+W cannot be
 * intercepted, so the menu is their discoverable home). Every close routes
 * through the dirty-tab confirm guard; a declined confirm skips that tab and
 * continues with the rest.
 */
const TabContextMenu: Component<{
  groupId: () => string;
  tab: TabType;
  children: JSX.Element;
}> = (props) => {
  const closeWith = (mode: TabCloseMode) => {
    const group = windowStore.tabGroups[props.groupId()];
    if (!group) return;
    for (const t of tabsToClose(group.tabs, props.tab.id, mode)) {
      if (confirmTabClose(t)) windowActions.removeTab(props.groupId(), t.id);
    }
  };
  return (
    <Menu.Root onSelect={(d) => closeWith(d.value as TabCloseMode)}>
      {/* asChild div: the default trigger is a BUTTON and TabItem carries its
          own close button — button-in-button is invalid HTML. */}
      <Menu.ContextTrigger
        asChild={(triggerProps) => (
          <div {...triggerProps({ class: 'contents' })}>{props.children}</div>
        )}
      />
      {/* Portaled: an in-flow positioner adds phantom layout inside the tab
          strip (it scrolled the tabs and broke pointer hit-testing). */}
      <Portal>
        <Menu.Positioner>
          <Menu.Content class="min-w-[10rem] rounded border border-hairline bg-surface-elevated py-1 text-xs text-shell-ink shadow-lg focus:outline-none z-50">
          <Menu.Item
            value="close"
            class="flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash"
          >
            Close
          </Menu.Item>
          <Menu.Item
            value="close-others"
            class="flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash"
          >
            Close Others
          </Menu.Item>
          <Menu.Item
            value="close-right"
            class="flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash"
          >
            Close to the Right
          </Menu.Item>
          </Menu.Content>
        </Menu.Positioner>
      </Portal>
    </Menu.Root>
  );
};

// ── Center TabBar ───────────────────────────────────────────────────────

const CenterTabBar: Component<{
  groupId: string;
  paneId: string;
  onPopOut?: () => void;
}> = (props) => {
  const group = () => windowStore.tabGroups[props.groupId];
  const tabs = () => group()?.tabs ?? [];
  const activeTabId = () => group()?.activeTabId ?? null;
  const isFocused = () => windowStore.activePaneId === props.paneId && windowStore.focusedRegion === 'center';

  let tabsContainerRef: HTMLDivElement | undefined;

  const droppable = createDroppable(`tabgroup:${props.groupId}`, {
    type: 'tabGroup',
    groupId: props.groupId,
  });

  const { insertIdx } = useTabBarDnD({
    groupId: () => props.groupId,
    tabsContainerRef: () => tabsContainerRef,
  });

  return (
    <div
      use:droppable
      ref={attachNativeMenuGuard}
      classList={{
        'flex-shrink-0 flex items-center h-9 bg-shell-bg border-b border-hairline relative': true,
        'bg-primary/5': droppable.isActiveDroppable,
      }}
    >
      <TabStrip
        tabs={tabs}
        activeTabId={activeTabId}
        insertIdx={insertIdx}
        onSelectTab={(tabId) => windowActions.setActiveTab(props.groupId, tabId)}
        onTabsContainerRef={(el) => {
          tabsContainerRef = el;
        }}
        renderTab={(tab) => (
          <TabContextMenu groupId={() => props.groupId} tab={tab()}>
            <TabItem
              tab={tab()}
              draggableId={`tab:${props.groupId}:${tab().id}`}
              draggableData={{ type: 'tab', tab: tab(), sourceGroupId: props.groupId }}
              isActive={tab().id === activeTabId()}
              isFocused={isFocused()}
              onClick={() => windowActions.setActiveTab(props.groupId, tab().id)}
              onClose={() => confirmTabClose(tab()) && windowActions.removeTab(props.groupId, tab().id)}
            />
          </TabContextMenu>
        )}
      />
      <div class="flex-shrink-0 flex items-center gap-0.5 px-1">
        {props.onPopOut && tabs().length > 0 && (
          <button
            onClick={props.onPopOut}
            class="w-6 h-6 flex items-center justify-center rounded text-muted-dark hover:text-shell-body hover:bg-hover-wash transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
            title="Pop out to floating window"
            aria-label="Pop out to floating window"
          >
            <IconLayout class="w-4 h-4" />
          </button>
        )}
      </div>
      {droppable.isActiveDroppable && (
        <div class="absolute inset-x-0 bottom-0 h-0.5 bg-primary cru-anim-fade" />
      )}
    </div>
  );
};

// ── Edge TabBar ─────────────────────────────────────────────────────────

const EdgeTabBar: Component<{
  position: EdgePanelPosition;
}> = (props) => {
  const groupId = () => windowStore.edgePanels[props.position].tabGroupId;
  const group = () => windowStore.tabGroups[groupId()];
  const tabs = () => group()?.tabs ?? [];
  const activeTabId = () => group()?.activeTabId ?? null;
  const isFocused = () => windowStore.focusedRegion === props.position;

  let containerRef: HTMLDivElement | undefined;
  let tabsContainerRef: HTMLDivElement | undefined;

  const droppable = createDroppable(`edgepanel:${props.position}`, {
    type: 'edgePanel',
    panelId: props.position,
  });

  const { insertIdx } = useTabBarDnD({
    groupId,
    tabsContainerRef: () => tabsContainerRef,
  });

  return (
    <div
      use:droppable
      ref={containerRef}
      data-testid={`edge-tabbar-${props.position}`}
      classList={{
        'flex-shrink-0 flex items-center h-9 bg-shell-bg border-b border-hairline relative': true,
        'bg-primary/5': droppable.isActiveDroppable,
      }}
    >
      <TabStrip
        tabs={tabs}
        activeTabId={activeTabId}
        insertIdx={insertIdx}
        onSelectTab={(tabId) => windowActions.setActiveTab(groupId(), tabId)}
        onTabsContainerRef={(el) => {
          tabsContainerRef = el;
        }}
        renderTab={(tab) => (
          <TabContextMenu groupId={groupId} tab={tab()}>
            <TabItem
              tab={tab()}
              draggableId={`edgetab:${props.position}:${tab().id}`}
              draggableData={{ type: 'tab', tab: tab(), sourceGroupId: groupId() }}
              isActive={activeTabId() === tab().id}
              isFocused={isFocused()}
              onClick={() => windowActions.setActiveTab(groupId(), tab().id)}
              onClose={() => confirmTabClose(tab()) && windowActions.removeTab(groupId(), tab().id)}
              testId={`edge-tab-${props.position}-${tab().id}`}
            />
          </TabContextMenu>
        )}
      />
      {droppable.isActiveDroppable && (
        <div class="absolute inset-x-0 bottom-0 h-0.5 bg-primary cru-anim-fade" />
      )}
    </div>
  );
};

// ── Exported TabBar (discriminated union dispatcher) ────────────────────

export const TabBar: Component<TabBarProps> = (props) => {
  if (props.mode === 'center') {
    // Keyed: CenterTabBar registers its `tabgroup:` droppable with the group
    // id captured at mount. Layout restores and group churn swap the id under
    // a surviving instance, leaving a stale drop target — remount instead.
    return (
      <Show when={props.mode === 'center' ? props.groupId : undefined} keyed>
        {(groupId) => (
          <CenterTabBar
            groupId={groupId}
            paneId={(props as Extract<TabBarProps, { mode: 'center' }>).paneId}
            onPopOut={(props as Extract<TabBarProps, { mode: 'center' }>).onPopOut}
          />
        )}
      </Show>
    );
  }
  // Keyed on the live group id for the same reason as CenterTabBar: layout
  // restores swap the edge panel's group id under a surviving instance, and
  // both the droppable and every TabItem draggable hold registration-time
  // snapshots of it — drags from a stale bar silently no-op in moveTab.
  const position = props.position;
  return (
    <Show when={windowStore.edgePanels[position].tabGroupId} keyed>
      {(_groupId) => <EdgeTabBar position={position} />}
    </Show>
  );
};
