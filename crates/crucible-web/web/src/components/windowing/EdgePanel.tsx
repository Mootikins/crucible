import { Component, Show, For, createSignal, onCleanup } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { EdgePanelPosition, EdgePanelTab } from '@/types/windowTypes';
import {
  IconPanelLeft,
  IconPanelLeftClose,
  IconPanelRight,
  IconPanelRightClose,
  IconPanelBottom,
  IconPanelBottomClose,
  IconGripVertical,
  IconGripHorizontal,
} from './icons';

const EDGE_PANEL_MIN_WIDTH = 120;
const EDGE_PANEL_MAX_WIDTH = 600;
const EDGE_PANEL_MIN_HEIGHT = 100;
const EDGE_PANEL_MAX_HEIGHT = 500;

function EdgePanelResizeHandle(props: { position: EdgePanelPosition }) {
  const panel = () => windowStore.edgePanels[props.position];
  const isVertical = () =>
    props.position === 'left' || props.position === 'right';
  let cleanup: (() => void) | null = null;

  onCleanup(() => {
    if (cleanup) {
      cleanup();
      cleanup = null;
    }
  });

  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startY = e.clientY;
    const startSize = isVertical()
      ? panel().width ?? 250
      : panel().height ?? 200;

    const handleMouseMove = (e: MouseEvent) => {
      if (props.position === 'left') {
        const delta = e.clientX - startX;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(EDGE_PANEL_MAX_WIDTH, startSize + delta))
        );
      } else if (props.position === 'right') {
        const delta = startX - e.clientX;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(EDGE_PANEL_MAX_WIDTH, startSize + delta))
        );
      } else {
        const delta = startY - e.clientY;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_HEIGHT, Math.min(EDGE_PANEL_MAX_HEIGHT, startSize + delta))
        );
      }
    };

    const handleMouseUp = () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      cleanup = null;
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    cleanup = handleMouseUp;
  };

  return (
    <div
      role="separator"
      aria-orientation={isVertical() ? 'vertical' : 'horizontal'}
      classList={{
        'group relative flex-shrink-0 bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500 transition-colors cursor-col-resize': isVertical(),
        'group relative flex-shrink-0 bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500 transition-colors cursor-row-resize': !isVertical(),
        'w-1': isVertical(),
        'h-1': !isVertical(),
      }}
      style={isVertical() ? { minWidth: '4px' } : { minHeight: '4px' }}
      onMouseDown={handleMouseDown}
    >
      <div
        classList={{
          'absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-zinc-500': true,
        }}
      >
        {isVertical() ? (
          <IconGripVertical class="w-3 h-6" />
        ) : (
          <IconGripHorizontal class="w-6 h-3" />
        )}
      </div>
    </div>
  );
}

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
          'flex-row': isVertical(),
          'flex-col': !isVertical(),
        }}
      >
        {props.position === 'right' && <EdgePanelResizeHandle position={props.position} />}
        {props.position === 'bottom' && <EdgePanelResizeHandle position={props.position} />}
        <div
          classList={{
            'flex flex-col overflow-hidden': true,
            'border-r border-zinc-800': isVertical(),
            'border-t border-zinc-800': !isVertical(),
          }}
          style={
            isVertical()
              ? { width: panel().width ? `${panel().width}px` : '250px', minWidth: 0 }
              : { height: panel().height ? `${panel().height}px` : '200px', minHeight: 0 }
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
        {props.position === 'left' && <EdgePanelResizeHandle position={props.position} />}
      </div>
    </Show>
  );
};
