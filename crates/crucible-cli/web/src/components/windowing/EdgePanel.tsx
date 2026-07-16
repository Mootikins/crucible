import { Component, Show, createEffect, onCleanup } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { Key } from '@solid-primitives/keyed';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { EdgePanelPosition, Tab } from '@/types/windowTypes';
import {
  IconPanelLeft,
  IconPanelRight,
  IconPanelBottom,
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

  // 1px visible line; the after: pseudo extends the pointer target ±4px so
  // the thin separator is still comfortable to grab (Obsidian-style).
  return (
    <div
      role="separator"
      aria-orientation={isVertical() ? 'vertical' : 'horizontal'}
      classList={{
        'relative flex-shrink-0 z-10 bg-zinc-800 hover:bg-zinc-600 active:bg-primary transition-colors after:content-[\'\'] after:absolute': true,
        'w-px cursor-col-resize after:inset-y-0 after:-inset-x-1': isVertical(),
        'h-px cursor-row-resize after:inset-x-0 after:-inset-y-1': !isVertical(),
      }}
      on:pointerdown={handlePointerDown}
    />
  );
}

/** One icon in the collapsed strip. Draggable with the same payload as an
 * expanded tab row, so a collapsed pane's tabs can be dragged to any drop
 * target without expanding first. */
const CollapsedTabButton: Component<{
  position: EdgePanelPosition;
  tab: Tab;
  groupId: string;
  isActive: boolean;
  isVertical: boolean;
}> = (props) => {
  const draggable = createDraggable(
    `edgetab-collapsed:${props.position}:${props.tab.id}`,
    { type: 'tab', tab: props.tab, sourceGroupId: props.groupId },
  );

  // Mirror TabItem: an open flyout must not linger over the drag.
  createEffect(() => {
    if (draggable.isActiveDraggable && windowStore.flyoutState?.isOpen) {
      windowActions.closeFlyout();
    }
  });

  return (
    <button
      use:draggable
      type="button"
      data-testid={`collapsed-tab-button-${props.position}`}
      classList={{
        'flex items-center justify-center transition-all duration-150': true,
        'w-10 h-10': props.isVertical,
        'h-9 px-3': !props.isVertical,
        'opacity-40': draggable.isActiveDraggable,
        'bg-zinc-800 text-zinc-100': props.isActive && !draggable.isActiveDraggable,
        'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50':
          !props.isActive && !draggable.isActiveDraggable,
      }}
      title={props.tab.title}
      onClick={() => {
        windowActions.setActiveTab(props.groupId, props.tab.id);
        windowActions.openFlyout(props.position, props.tab.id);
      }}
    >
      {props.tab.icon ? (
        <props.tab.icon class="w-4 h-4" />
      ) : (
        <span class="text-xs truncate max-w-[2rem]">{props.tab.title[0]}</span>
      )}
    </button>
  );
};

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
        // Border faces the center, matching the expanded panel's separator.
        'flex-col border-r': props.position === 'left',
        'flex-col border-l': props.position === 'right',
        'flex-row border-t': !isVertical(),
        'bg-primary/20': droppable.isActiveDroppable,
      }}
    >
      {/* Expand control keeps a fixed home: the h-9 top slot mirrors the tab
          bar row it replaces on vertical panels; on the bottom strip it pins
          to the right end so it doesn't drift as tabs come and go. */}
      {/* Toggle glyphs are w-4 everywhere (header, strips, tab bars) —
          Lucide's bare default is 24px and reads as a different control. */}
      <button
        type="button"
        data-testid={`edge-expand-${props.position}`}
        classList={{
          'flex items-center justify-center text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors': true,
          'w-10 h-9 border-b border-zinc-800': isVertical(),
          'order-last ml-auto h-9 px-2': !isVertical(),
        }}
        title="Expand panel"
        onClick={() => windowActions.toggleEdgePanel(props.position)}
      >
        {props.position === 'left' && <IconPanelLeft class="w-4 h-4" />}
        {props.position === 'right' && <IconPanelRight class="w-4 h-4" />}
        {props.position === 'bottom' && <IconPanelBottom class="w-4 h-4" />}
      </button>
      {/* Outer Show keyed on the group id: a layout restore swaps group ids
          under surviving components, and solid-dnd draggable data is a
          registration-time snapshot — without a remount every drag would
          carry a dead sourceGroupId and moveTab would silently no-op.
          Inner Key by tab id: updateTab replaces the tab object on every
          write, and a remounting row re-registers its draggable under the
          same id, leaving it silently undraggable (same trap as TabStrip). */}
      <Show when={panel().tabGroupId} keyed>
        {(groupId) => (
          <Key each={tabs()} by={(t) => t.id}>
            {(tab) => (
              <CollapsedTabButton
                position={props.position}
                tab={tab()}
                groupId={groupId}
                isActive={activeTabId() === tab().id}
                isVertical={isVertical()}
              />
            )}
          </Key>
        )}
      </Show>
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
        {/* No border here — the resize handle's 1px line is the separator. */}
        <div
          class="flex flex-col overflow-hidden"
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
            {(() => {
              const tab = activeTab();
              if (!tab) return <span>Select a tab</span>;
              const panel = getGlobalRegistry().get(tab.contentType);
              if (panel) {
                const panelProps = (tab.metadata ?? {}) as Record<string, unknown>;
                return <Dynamic component={panel.component} {...panelProps} />;
              }
              return <div>{tab.title} content</div>;
            })()}
          </div>
        </div>
        {props.position === 'left' && <EdgePanelResizeHandle position={props.position} />}
      </div>
    </Show>
  );
};
