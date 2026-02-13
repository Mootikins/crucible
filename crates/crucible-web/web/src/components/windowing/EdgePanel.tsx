import { Component, Show, For } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { EdgePanelPosition, EdgePanelTab } from '@/types/windowTypes';
import {
  IconPanelLeft,
  IconPanelLeftClose,
  IconPanelRight,
  IconPanelRightClose,
  IconPanelBottom,
  IconPanelBottomClose,
} from './icons';

export const EdgePanel: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const isCollapsed = () => panel().isCollapsed;
  const isVertical = () => props.position === 'left' || props.position === 'right';
  const activeTab = () => {
    const p = panel();
    const tab = p.tabs.find((t) => t.id === p.activeTabId);
    return tab ?? null;
  };

  const PanelIcon = () => {
    if (props.position === 'left')
      return isCollapsed() ? <IconPanelLeft /> : <IconPanelLeftClose />;
    if (props.position === 'right')
      return isCollapsed() ? <IconPanelRight /> : <IconPanelRightClose />;
    return isCollapsed() ? <IconPanelBottom /> : <IconPanelBottomClose />;
  };

  return (
    <Show
      when={!isCollapsed()}
      fallback={
        <div
          classList={{
            'flex bg-zinc-900/95 border-zinc-800': true,
            'flex-col border-r': isVertical(),
            'flex-row border-t': !isVertical(),
          }}
        >
          <For each={panel().tabs}>
            {(tab) => (
              <button
                type="button"
                classList={{
                  'flex items-center justify-center transition-all duration-150': true,
                  'w-10 h-10': isVertical(),
                  'h-9 px-3': !isVertical(),
                  'bg-zinc-800 text-zinc-100': panel().activeTabId === tab.id,
                  'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50':
                    panel().activeTabId !== tab.id,
                }}
                title={tab.title}
                onClick={() => windowActions.setEdgePanelActiveTab(props.position, tab.id)}
              >
                <span class="text-xs truncate max-w-[2rem]">{tab.title[0]}</span>
              </button>
            )}
          </For>
          <button
            type="button"
            classList={{
              'flex items-center justify-center text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors': true,
              'w-10 h-10 mt-auto': isVertical(),
              'h-9 px-2': !isVertical(),
            }}
            title="Expand panel"
            onClick={() => windowActions.toggleEdgePanel(props.position)}
          >
            {props.position === 'left' && <IconPanelLeft />}
            {props.position === 'right' && <IconPanelRight />}
            {props.position === 'bottom' && <IconPanelBottom />}
          </button>
        </div>
      }
    >
      <div
        classList={{
          'flex bg-zinc-900/95 border-zinc-800 overflow-hidden': true,
          'flex-col border-r': isVertical(),
          'flex-row border-t': !isVertical(),
          'w-[250px]': isVertical(),
          'h-[200px]': !isVertical(),
        }}
        style={
          isVertical()
            ? { width: panel().width ? `${panel().width}px` : '250px' }
            : { height: panel().height ? `${panel().height}px` : '200px' }
        }
      >
        <div
          classList={{
            'flex border-b border-zinc-800': true,
            'flex-row': isVertical(),
            'flex-col': !isVertical(),
          }}
        >
          <For each={panel().tabs}>
            {(tab) => (
              <button
                type="button"
                classList={{
                  'group relative flex items-center gap-1.5 transition-all duration-150 cursor-pointer': true,
                  'flex-row px-2 py-1.5 border-b border-zinc-800': isVertical(),
                  'flex-row px-2 py-1.5': !isVertical(),
                  'bg-zinc-800 text-zinc-100': panel().activeTabId === tab.id,
                  'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50':
                    panel().activeTabId !== tab.id,
                  'border-b-2 border-blue-500': !isVertical() && panel().activeTabId === tab.id,
                }}
                onClick={() => windowActions.setEdgePanelActiveTab(props.position, tab.id)}
              >
                <span class="text-xs font-medium truncate max-w-[100px]">{tab.title}</span>
              </button>
            )}
          </For>
        </div>
        <div class="flex-1 overflow-auto p-2 text-xs text-zinc-400">
          <Show when={activeTab()} fallback={<span>Select a tab</span>}>
            {(tab) => <div>{tab().title} content</div>}
          </Show>
        </div>
      </div>
    </Show>
  );
};
