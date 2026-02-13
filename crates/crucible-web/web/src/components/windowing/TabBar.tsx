import { Component, For, Show, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import {
  createDraggable,
  createDroppable,
} from '@thisbeyond/solid-dnd';
import type { Tab as TabType } from '@/types/windowTypes';
import { windowStore, windowActions } from '@/stores/windowStore';
import { IconGripVertical, IconClose, IconLayout } from './icons';

const Tab: Component<{
  tab: TabType;
  groupId: string;
  isActive: boolean;
  isFocused: boolean;
  onClose: (e: MouseEvent) => void;
}> = (props) => {
  const id = () => `tab:${props.groupId}:${props.tab.id}`;
  const draggable = createDraggable(id(), {
    type: 'tab',
    tab: props.tab,
    sourceGroupId: props.groupId,
  });
  const Icon = props.tab.icon;

  return (
    <div
      use:draggable
      data-tab-id={props.tab.id}
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
      onClick={() => {
        windowActions.setActiveTab(props.groupId, props.tab.id);
      }}
      onMouseEnter={() => {}}
      onMouseLeave={() => {}}
    >
      <div class="cursor-grab active:cursor-grabbing p-0.5 -ml-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <IconGripVertical class="w-3 h-3 text-zinc-500" />
      </div>
      {Icon && (
        <Icon
          class={`w-3.5 h-3.5 flex-shrink-0 ${props.isActive ? 'text-zinc-300' : 'text-zinc-500'}`}
        />
      )}
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
        class="flex-shrink-0 p-0.5 rounded-sm transition-all hover:bg-zinc-700 hover:text-zinc-200 opacity-0 group-hover:opacity-100 focus:opacity-100"
      >
        <IconClose class="w-3 h-3" />
      </button>
    </div>
  );
};

export const TabBar: Component<{
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
  let tabsContainerRef: HTMLDivElement | undefined;

  const droppable = createDroppable(`tabgroup:${props.groupId}`, {
    type: 'tabGroup',
    groupId: props.groupId,
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
          {(tab) => (
            <Tab
              tab={tab}
              groupId={props.groupId}
              isActive={tab.id === activeTabId()}
              isFocused={isFocused()}
              onClose={() => windowActions.removeTab(props.groupId, tab.id)}
            />
          )}
        </For>
      </div>
      <Show when={isOverflowing()}>
        <div class="relative flex-shrink-0">
          <button
            class="flex-shrink-0 w-6 h-6 flex items-center justify-center text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded transition-colors text-xs"
            onClick={(e) => { e.stopPropagation(); setShowDropdown(!showDropdown()); }}
            title="Show all tabs"
          >
            ▼
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

