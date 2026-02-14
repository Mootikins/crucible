import { Component, For, Show, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import {
  createDraggable,
  createDroppable,
  useDragDropContext,
} from '@thisbeyond/solid-dnd';
import type { Tab as TabType, EdgePanelPosition, TabBarProps, DragSource } from '@/types/windowTypes';
import { windowStore, windowActions } from '@/stores/windowStore';
import { IconGripVertical, IconClose, IconLayout } from './icons';
import { ChevronDown } from '@/lib/icons';

// ── Module-level reorder state (shared with WindowManager) ──────────────

export type ReorderState = {
  groupId: string;
  insertIndex: number;
} | null;

const [reorderState, setReorderState] = createSignal<ReorderState>(null);
export { reorderState, setReorderState };

// ── Insert-index computation helper ─────────────────────────────────────

function computeInsertIndex(
  containerEl: HTMLElement,
  pointerX: number,
  draggedTabId?: string,
): number | null {
  const tabEls = containerEl.querySelectorAll('[data-tab-id]');
  let adjustedIndex = 0;
  for (let i = 0; i < tabEls.length; i++) {
    const el = tabEls[i] as HTMLElement;
    if (draggedTabId && el.dataset.tabId === draggedTabId) continue;
    const rect = el.getBoundingClientRect();
    if (pointerX < rect.left + rect.width / 2) return adjustedIndex;
    adjustedIndex++;
  }
  return adjustedIndex;
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
  onDragStart?: () => void;
}

const TabItem: Component<TabItemProps> = (props) => {
  const draggable = createDraggable(props.draggableId, props.draggableData);
  const Icon = props.tab.icon;

  // Fire onDragStart callback when this tab becomes the active draggable
  createEffect(() => {
    if (draggable.isActiveDraggable) {
      props.onDragStart?.();
    }
  });

  return (
    <div
      use:draggable
      data-tab-id={props.tab.id}
      {...(props.testId ? { 'data-testid': props.testId } : {})}
      classList={{
        'group relative flex items-center gap-1 px-2.5 py-1.5 cursor-pointer transition-all duration-100 border-b-2 rounded-t-sm':
          true,
        'opacity-40 border-transparent bg-zinc-800/50': draggable.isActiveDraggable,
        'bg-zinc-800 text-zinc-100': props.isActive && !draggable.isActiveDraggable,
        'border-blue-500': props.isActive && props.isFocused && !draggable.isActiveDraggable,
        'border-zinc-600': props.isActive && !props.isFocused && !draggable.isActiveDraggable,
        'border-transparent text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50':
          !props.isActive && !draggable.isActiveDraggable,
      }}
      onClick={() => props.onClick()}
    >
      <div class="relative w-3.5 h-3.5 flex-shrink-0 cursor-grab active:cursor-grabbing">
        <Show when={Icon} fallback={
          <IconGripVertical class="w-3.5 h-3.5 text-zinc-500 opacity-0 group-hover:opacity-100 transition-opacity duration-150" />
        }>
          <>
            {Icon && <Icon class={`absolute inset-0 w-3.5 h-3.5 ${props.isActive ? 'text-zinc-300' : 'text-zinc-500'} group-hover:opacity-0 transition-opacity duration-150`} />}
            <IconGripVertical class="absolute inset-0 w-3.5 h-3.5 text-zinc-500 opacity-0 group-hover:opacity-100 transition-opacity duration-150" />
          </>
        </Show>
      </div>
      <span class="text-xs font-medium truncate max-w-[120px]">
        {props.tab.title}
      </span>
      {props.tab.isModified && (
        <span class="w-1.5 h-1.5 rounded-full bg-amber-500 flex-shrink-0" />
      )}
      <button
        onClick={(e) => {
          e.stopPropagation();
          props.onClose(e);
        }}
        classList={{
          'flex-shrink-0 p-0.5 rounded-sm transition-all hover:bg-zinc-700 hover:text-zinc-200 focus:opacity-100': true,
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
  <div class="w-0.5 h-5 bg-blue-500 rounded-full flex-shrink-0 my-auto" />
);

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

  const [isOverflowing, setIsOverflowing] = createSignal(false);
  const [showDropdown, setShowDropdown] = createSignal(false);
  const [insertIdx, setInsertIdx] = createSignal<number | null>(null);
  let tabsContainerRef: HTMLDivElement | undefined;

  const droppable = createDroppable(`tabgroup:${props.groupId}`, {
    type: 'tabGroup',
    groupId: props.groupId,
  });

  const dndCtx = useDragDropContext();

  const isSameBarDrag = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return false;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' && data.sourceGroupId === props.groupId;
  };

  const draggedTabId = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return undefined;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' ? data.tab.id : undefined;
  };

  // Read sensor coordinates reactively — onPointerMove doesn't fire during
  // drag because the DragOverlay portal intercepts pointer events.
  createEffect(() => {
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
      const inBounds = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
      if (!inBounds) {
        setInsertIdx(null);
        setReorderState(null);
        return;
      }
      const idx = computeInsertIndex(tabsContainerRef, x, draggedTabId());
      setInsertIdx(idx);
      if (idx != null) {
        setReorderState({ groupId: props.groupId, insertIndex: idx });
      }
    } else {
      setInsertIdx(null);
    }
  });

  createEffect(() => {
    if (!dndCtx?.[0]?.active?.draggable) {
      setInsertIdx(null);
      setReorderState(null);
    }
  });

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
      tabs();
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
    <div
      use:droppable
      classList={{
        'flex items-center h-9 bg-zinc-900 border-b border-zinc-800 relative': true,
        'bg-blue-500/5': droppable.isActiveDroppable,
      }}
    >
      <div
        ref={tabsContainerRef}
        class="flex-1 flex items-end gap-0.5 overflow-x-auto scrollbar-hide px-1 min-w-0 [scrollbar-width:none] [-ms-overflow-style:none]"
      >
        <For each={tabs()}>
          {(tab, i) => (
            <>
              <Show when={insertIdx() === i()}>
                <InsertIndicator />
              </Show>
              <TabItem
                tab={tab}
                draggableId={`tab:${props.groupId}:${tab.id}`}
                draggableData={{ type: 'tab', tab, sourceGroupId: props.groupId }}
                isActive={tab.id === activeTabId()}
                isFocused={isFocused()}
                onClick={() => windowActions.setActiveTab(props.groupId, tab.id)}
                onClose={() => windowActions.removeTab(props.groupId, tab.id)}
              />
            </>
          )}
        </For>
        <Show when={insertIdx() === tabs().length}>
          <InsertIndicator />
        </Show>
      </div>
       <Show when={isOverflowing()}>
         <div class="relative flex-shrink-0">
           <button
             class="flex-shrink-0 w-6 h-6 flex items-center justify-center text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors"
             onClick={(e) => { e.stopPropagation(); setShowDropdown(!showDropdown()); }}
             title="Show all tabs"
           >
             <ChevronDown class="w-3.5 h-3.5" />
           </button>
          <Show when={showDropdown()}>
            <div class="absolute right-0 top-full mt-1 z-50 min-w-[160px] max-w-[280px] bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl py-1 max-h-[300px] overflow-y-auto">
              <For each={tabs()}>
                {(tab) => (
                  <button
                    class={`w-full px-3 py-1.5 text-left text-xs truncate transition-colors ${
                      tab.id === activeTabId()
                        ? 'bg-blue-500/20 text-blue-300 font-medium'
                        : 'text-zinc-300 hover:bg-zinc-700'
                    }`}
                    onClick={() => {
                      windowActions.setActiveTab(props.groupId, tab.id);
                      setShowDropdown(false);
                      const tabEl = tabsContainerRef?.querySelector(`[data-tab-id="${tab.id}"]`);
                      tabEl?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
                    }}
                  >
                    {tab.title}
                    {tab.isModified && <span class="ml-1 text-amber-500">●</span>}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
      <div class="flex-shrink-0 flex items-center gap-0.5 px-1">
        {props.onPopOut && tabs().length > 0 && (
          <button
            onClick={props.onPopOut}
            class="w-6 h-6 flex items-center justify-center rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
            title="Pop out to floating window"
          >
            <IconLayout class="w-3 h-3" />
          </button>
        )}
      </div>
      {droppable.isActiveDroppable && (
        <div class="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />
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

  const [isOverflowing, setIsOverflowing] = createSignal(false);
  const [showDropdown, setShowDropdown] = createSignal(false);
  const [insertIdx, setInsertIdx] = createSignal<number | null>(null);
  let containerRef: HTMLDivElement | undefined;
  let tabsContainerRef: HTMLDivElement | undefined;

  const droppable = createDroppable(`edgepanel:${props.position}`, {
    type: 'edgePanel',
    panelId: props.position,
  });

  const dndCtx = useDragDropContext();

  const isSameBarDrag = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return false;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' && data.sourceGroupId === groupId();
  };

  const draggedTabId = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return undefined;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' ? data.tab.id : undefined;
  };

  // Read sensor coordinates reactively — onPointerMove doesn't fire during
  // drag because the DragOverlay portal intercepts pointer events.
  createEffect(() => {
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
      const inBounds = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
      if (!inBounds) {
        setInsertIdx(null);
        setReorderState(null);
        return;
      }
      const idx = computeInsertIndex(tabsContainerRef, x, draggedTabId());
      setInsertIdx(idx);
      if (idx != null) {
        setReorderState({ groupId: groupId(), insertIndex: idx });
      }
    } else {
      setInsertIdx(null);
    }
  });

  createEffect(() => {
    if (!dndCtx?.[0]?.active?.draggable) {
      setInsertIdx(null);
      setReorderState(null);
    }
  });

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
      tabs();
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
    <div
      use:droppable
      ref={containerRef}
      data-testid={`edge-tabbar-${props.position}`}
      classList={{
        'flex items-center h-9 bg-zinc-900 border-b border-zinc-800 relative': true,
        'bg-blue-500/5': droppable.isActiveDroppable,
      }}
    >
      <div
        ref={tabsContainerRef}
        class="flex-1 flex items-end gap-0.5 overflow-x-auto scrollbar-hide px-1 min-w-0 [scrollbar-width:none] [-ms-overflow-style:none]"
      >
        <For each={tabs()}>
          {(tab, i) => (
            <>
              <Show when={insertIdx() === i()}>
                <InsertIndicator />
              </Show>
              <TabItem
                tab={tab}
                draggableId={`edgetab:${props.position}:${tab.id}`}
                draggableData={{ type: 'tab', tab, sourceGroupId: groupId() }}
                isActive={activeTabId() === tab.id}
                isFocused={isFocused()}
                onClick={() => windowActions.setActiveTab(groupId(), tab.id)}
                onClose={() => windowActions.removeTab(groupId(), tab.id)}
                testId={`edge-tab-${props.position}-${tab.id}`}
                onDragStart={() => { if (windowStore.flyoutState?.isOpen) windowActions.closeFlyout(); }}
              />
            </>
          )}
        </For>
        <Show when={insertIdx() === tabs().length}>
          <InsertIndicator />
        </Show>
      </div>
      <Show when={isOverflowing()}>
        <div class="relative flex-shrink-0">
          <button
            class="flex-shrink-0 w-6 h-6 flex items-center justify-center text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors"
            onClick={(e) => { e.stopPropagation(); setShowDropdown(!showDropdown()); }}
            title="Show all tabs"
          >
            <ChevronDown class="w-3.5 h-3.5" />
          </button>
          <Show when={showDropdown()}>
            <div class="absolute right-0 top-full mt-1 z-50 min-w-[160px] max-w-[280px] bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl py-1 max-h-[300px] overflow-y-auto">
              <For each={tabs()}>
                {(tab) => (
                  <button
                    class={`w-full px-3 py-1.5 text-left text-xs truncate transition-colors ${
                      tab.id === activeTabId()
                        ? 'bg-blue-500/20 text-blue-300 font-medium'
                        : 'text-zinc-300 hover:bg-zinc-700'
                    }`}
                    onClick={() => {
                      windowActions.setActiveTab(groupId(), tab.id);
                      setShowDropdown(false);
                      const tabEl = tabsContainerRef?.querySelector(`[data-tab-id="${tab.id}"]`);
                      tabEl?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
                    }}
                  >
                    {tab.title}
                    {tab.isModified && <span class="ml-1 text-amber-500">●</span>}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
      {droppable.isActiveDroppable && (
        <div class="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />
      )}
    </div>
  );
};

// ── Exported TabBar (discriminated union dispatcher) ────────────────────

export const TabBar: Component<TabBarProps> = (props) => {
  if (props.mode === 'center') {
    return (
      <CenterTabBar
        groupId={props.groupId}
        paneId={props.paneId}
        onPopOut={props.onPopOut}
      />
    );
  }
  return (
    <EdgeTabBar
      position={props.position}
    />
  );
};
