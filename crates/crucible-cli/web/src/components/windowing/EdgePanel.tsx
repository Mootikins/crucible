import { Component, Show, onCleanup } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { Key } from '@solid-primitives/keyed';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { EdgePanelPosition, Tab } from '@/types/windowTypes';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { openPanelTab } from '@/lib/panel-actions';
import { TabBar } from './TabBar';
import {
  IconPanelLeft,
  IconPanelLeftClose,
  IconPanelRight,
  IconPanelRightClose,
  IconPanelBottom,
  IconPanelBottomClose,
  IconZap,
  IconSettings,
} from './icons';
import { Plus } from '@/lib/icons';

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
        'relative flex-shrink-0 z-10 bg-control hover:bg-hover-wash active:bg-primary transition-colors after:content-[\'\'] after:absolute': true,
        'w-px cursor-col-resize after:inset-y-0 after:-inset-x-1': isVertical(),
        'h-px cursor-row-resize after:inset-x-0 after:-inset-y-1': !isVertical(),
      }}
      on:pointerdown={handlePointerDown}
    />
  );
}

/** One icon in the ribbon. Click toggles its panel (Obsidian-style: the
 * panel grows out of the always-visible bar); draggable with the same
 * payload as an expanded tab row, so a panel's tabs can be dragged to any
 * drop target without expanding first. */
const RibbonTabButton: Component<{
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

  const handleClick = () => {
    const panel = windowStore.edgePanels[props.position];
    if (panel.isCollapsed) {
      windowActions.setActiveTab(props.groupId, props.tab.id);
      windowActions.setEdgePanelCollapsed(props.position, false);
    } else if (props.isActive) {
      windowActions.setEdgePanelCollapsed(props.position, true);
    } else {
      windowActions.setActiveTab(props.groupId, props.tab.id);
    }
  };

  const highlighted = () =>
    props.isActive && !windowStore.edgePanels[props.position].isCollapsed;

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
        'bg-surface-elevated text-shell-ink': highlighted() && !draggable.isActiveDraggable,
        'text-muted-dark hover:text-shell-body hover:bg-hover-wash':
          !highlighted() && !draggable.isActiveDraggable,
      }}
      title={props.tab.title}
      onClick={handleClick}
    >
      {props.tab.icon ? (
        <props.tab.icon class="w-4 h-4" />
      ) : (
        <span class="text-xs truncate max-w-[2rem]">{props.tab.title[0]}</span>
      )}
    </button>
  );
};

const ribbonBtn =
  'flex items-center justify-center text-muted-dark hover:text-shell-body hover:bg-hover-wash transition-colors';

/** One command button on the ribbon (opens a modal/panel — Obsidian puts
 * these on the ribbon: palette, quick actions, settings gear at bottom). */
const RibbonCommand: Component<{
  title: string;
  testId: string;
  onClick: () => void;
  bottom?: boolean;
  children: ReturnType<Component>;
}> = (props) => (
  <button
    type="button"
    data-testid={props.testId}
    classList={{ [`${ribbonBtn} w-10 h-9 flex-none`]: true, 'mt-auto': props.bottom }}
    title={props.title}
    onClick={() => props.onClick()}
  >
    {props.children}
  </button>
);

/** The always-visible icon bar at the window edge (Obsidian's ribbon):
 * panels grow out of it, so the toggles never move or disappear. The top
 * (or leading, for the bottom bar) button expands/collapses the panel. */
const EdgeRibbon: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const group = () => windowStore.tabGroups[panel().tabGroupId];
  const tabs = () => group()?.tabs ?? [];
  const activeTabId = () => group()?.activeTabId ?? null;
  const isVertical = () => props.position === 'left' || props.position === 'right';

  const droppable = createDroppable(`edgepanel-collapsed:${props.position}`, {
    type: 'edgePanel',
    panelId: props.position,
  });

  const toggleIcon = () => {
    const collapsed = panel().isCollapsed;
    switch (props.position) {
      case 'left':
        return collapsed ? <IconPanelLeft class="w-4 h-4" /> : <IconPanelLeftClose class="w-4 h-4" />;
      case 'right':
        return collapsed ? <IconPanelRight class="w-4 h-4" /> : <IconPanelRightClose class="w-4 h-4" />;
      default:
        return collapsed ? <IconPanelBottom class="w-4 h-4" /> : <IconPanelBottomClose class="w-4 h-4" />;
    }
  };

  return (
    <div
      use:droppable
      data-testid={`edge-collapsed-drop-${props.position}`}
      classList={{
        'flex bg-shell-bg border-hairline transition-colors': true,
        // Border faces the center/panel it grows toward.
        'flex-col border-r': props.position === 'left',
        'flex-col border-l': props.position === 'right',
        'flex-row border-t': !isVertical(),
        'bg-primary/20': droppable.isActiveDroppable,
      }}
    >
      {/* Top/leading slot: this bar's panel toggle — always in view. */}
      <button
        type="button"
        data-testid={`ribbon-toggle-${props.position}`}
        classList={{
          [`${ribbonBtn} flex-none`]: true,
          'w-10 h-9 border-b border-hairline': isVertical(),
          'h-9 px-2 border-r border-hairline': !isVertical(),
        }}
        title={panel().isCollapsed ? 'Expand panel' : 'Collapse panel'}
        onClick={() => windowActions.toggleEdgePanel(props.position)}
      >
        {toggleIcon()}
      </button>
      <Show when={props.position === 'left'}>
        <RibbonCommand
          title="Command palette (Ctrl+P)"
          testId="ribbon-cmd-palette"
          onClick={() => window.dispatchEvent(new CustomEvent('crucible:open-command-palette'))}
        >
          <IconZap class="w-4 h-4" />
        </RibbonCommand>
        <RibbonCommand
          title="New session"
          testId="ribbon-cmd-new-session"
          onClick={() => window.dispatchEvent(new CustomEvent('crucible:new-session'))}
        >
          <Plus class="w-4 h-4" />
        </RibbonCommand>
        <div class="mx-2 my-1 h-px flex-none bg-hairline" />
      </Show>
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
              <RibbonTabButton
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
      <Show when={props.position === 'left'}>
        {/* Settings pinned at the ribbon's bottom, like Obsidian's gear. */}
        <RibbonCommand
          title="Open Settings"
          testId="ribbon-cmd-settings"
          bottom
          onClick={() => openPanelTab('settings')}
        >
          <IconSettings class="w-4 h-4" />
        </RibbonCommand>
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

  const expandedPanel = () => (
    <>
      {(props.position === 'right' || props.position === 'bottom') && (
        <EdgePanelResizeHandle position={props.position} />
      )}
      {/* No border here — the ribbon and handle lines are the separators. */}
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
        <div class="flex-1 overflow-auto p-2 text-xs text-muted" data-testid={`panel-content-${activeTab()?.contentType ?? 'unknown'}`}>
          {(() => {
            const tab = activeTab();
            if (!tab) return <span>Select a tab</span>;
            const panelDef = getGlobalRegistry().get(tab.contentType);
            if (panelDef) {
              const panelProps = (tab.metadata ?? {}) as Record<string, unknown>;
              return <Dynamic component={panelDef.component} {...panelProps} />;
            }
            return <div>{tab.title} content</div>;
          })()}
        </div>
      </div>
      {props.position === 'left' && <EdgePanelResizeHandle position={props.position} />}
    </>
  );

  // Ribbon at the window edge, always; the panel grows out of it toward the
  // center. Left: [ribbon][panel][handle]; right: [handle][panel][ribbon];
  // bottom: [handle][panel] over [ribbon].
  return (
    <div
      classList={{
        'flex bg-shell-bg overflow-hidden': true,
        'flex-row': isVertical(),
        'flex-col': !isVertical(),
      }}
    >
      {props.position === 'left' && <EdgeRibbon position="left" />}
      <Show when={!isCollapsed()}>
        <div
          classList={{
            'flex overflow-hidden': true,
            'flex-row': isVertical(),
            'flex-col': !isVertical(),
            // Panels grow out of their ribbon — slide in from the owning edge.
            'cru-anim-slide-l': props.position === 'left',
            'cru-anim-slide-r': props.position === 'right',
            'cru-anim-slide-b': props.position === 'bottom',
          }}
        >
          {expandedPanel()}
        </div>
      </Show>
      {props.position !== 'left' && <EdgeRibbon position={props.position} />}
    </div>
  );
};
