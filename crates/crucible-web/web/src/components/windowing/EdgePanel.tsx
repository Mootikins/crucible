import { Component, Show, For, onCleanup } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { EdgePanelPosition } from '@/types/windowTypes';
import {
  IconPanelLeft,
  IconPanelRight,
  IconPanelBottom,
  IconGripVertical,
  IconGripHorizontal,
} from './icons';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { TabBar } from './TabBar';

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

  const handlePointerDown = (e: PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startY = e.clientY;
    const startSize = isVertical()
      ? panel().width ?? 250
      : panel().height ?? 200;

    const handlePointerMove = (e: PointerEvent) => {
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

    const handlePointerUp = (e: PointerEvent) => {
      el.releasePointerCapture(e.pointerId);
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
      cleanup = null;
    };

    document.addEventListener('pointermove', handlePointerMove);
    document.addEventListener('pointerup', handlePointerUp);
    cleanup = () => {
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
    };
  };

  return (
    <div
      role="separator"
      aria-orientation={isVertical() ? 'vertical' : 'horizontal'}
      classList={{
        'group relative flex-shrink-0 bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500 transition-colors cursor-col-resize': isVertical(),
        'group relative flex-shrink-0 bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500 transition-colors cursor-row-resize': !isVertical(),
        'w-1.5': isVertical(),
        'h-1.5': !isVertical(),
      }}
      style={isVertical() ? { 'min-width': '6px' } : { 'min-height': '6px' }}
      on:pointerdown={handlePointerDown}
    >
      <div
        classList={{
          'absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-zinc-500': true,
        }}
      >
        {isVertical() ? (
          <IconGripVertical class="w-1 h-4" />
        ) : (
          <IconGripHorizontal class="w-4 h-1" />
        )}
      </div>
    </div>
  );
}

const CollapsedEdgeStrip: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const group = () => windowStore.tabGroups[panel().tabGroupId];
  const tabs = () => group()?.tabs ?? [];
  const activeTabId = () => group()?.activeTabId ?? null;
  const isVertical = () => props.position === 'left' || props.position === 'right';

  const droppable = createDroppable(`edgepanel-collapsed:${props.position}`, {
    type: 'edgePanel',
    panelId: props.position,
  });

  return (
    <div
      use:droppable
      data-testid={`edge-collapsed-drop-${props.position}`}
      classList={{
        'flex bg-zinc-900/95 border-zinc-800 transition-colors': true,
        'flex-col border-r': isVertical(),
        'flex-row border-t': !isVertical(),
        'bg-blue-500/20': droppable.isActiveDroppable,
      }}
    >
      <For each={tabs()}>
        {(tab) => (
          <button
            type="button"
            data-testid={`collapsed-tab-button-${props.position}`}
            classList={{
              'flex items-center justify-center transition-all duration-150': true,
              'w-10 h-10': isVertical(),
              'h-9 px-3': !isVertical(),
              'bg-zinc-800 text-zinc-100': activeTabId() === tab.id,
              'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50':
                activeTabId() !== tab.id,
            }}
            title={tab.title}
            onClick={() => {
              windowActions.setActiveTab(panel().tabGroupId, tab.id);
              windowActions.openFlyout(props.position, tab.id);
            }}
          >
            {tab.icon ? <tab.icon class="w-4 h-4" /> : <span class="text-xs truncate max-w-[2rem]">{tab.title[0]}</span>}
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
  );
};

export const EdgePanel: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const group = () => windowStore.tabGroups[panel().tabGroupId];
  const isCollapsed = () => panel().isCollapsed;
  const isVertical = () => props.position === 'left' || props.position === 'right';
  const activeTab = () => {
    const g = group();
    if (!g?.activeTabId) return null;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? null;
  };

  return (
    <Show
      when={!isCollapsed()}
      fallback={<CollapsedEdgeStrip position={props.position} />}
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
              ? { width: panel().width ? `${panel().width}px` : '250px', 'min-width': '0' }
              : { height: panel().height ? `${panel().height}px` : '200px', 'min-height': '0' }
          }
        >
          <TabBar
            mode="edge"
            position={props.position}
          />
          <div class="flex-1 overflow-auto p-2 text-xs text-zinc-400" data-testid={`panel-content-${activeTab()?.contentType ?? 'unknown'}`}>
            <Show when={activeTab()} fallback={<span>Select a tab</span>}>
              {(tab) => {
                const panel = getGlobalRegistry().get(tab().contentType);
                if (panel) {
                  return <Dynamic component={panel.component} />;
                }
                return <div>{tab().title} content</div>;
              }}
            </Show>
          </div>
        </div>
        {props.position === 'left' && <EdgePanelResizeHandle position={props.position} />}
      </div>
    </Show>
  );
};
