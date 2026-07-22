import { Component, Show, createEffect, createSignal, on, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions } from '@/stores/windowStore';
import { collectLeafGroupIds } from '@/stores/windowStoreInternals';
import type { EdgePanelPosition, Tab } from '@/types/windowTypes';
import { openPanelTab } from '@/lib/panel-actions';
import { SplitPane } from './SplitPane';
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
// No fixed max — an edge panel hosting a chat session should be able to take
// most of the viewport; just keep a sliver of center pane usable.
const edgePanelMaxWidth = () => Math.max(600, window.innerWidth - 320);
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
          Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(edgePanelMaxWidth(), startSize + delta))
        );
      } else if (props.position === 'right') {
        const delta = startX - e.clientX;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(edgePanelMaxWidth(), startSize + delta))
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
  const isVertical = () => props.position === 'left' || props.position === 'right';

  // The panel is a layout tree — the ribbon shows every tab across all leaf
  // groups, in tree order. Each leaf group keeps its own active tab (one
  // highlighted icon per split).
  const leafEntries = () => {
    const entries: { groupId: string; tab: Tab; isActive: boolean }[] = [];
    for (const gid of collectLeafGroupIds(panel().layout)) {
      const g = windowStore.tabGroups[gid];
      if (!g) continue;
      for (const t of g.tabs) {
        entries.push({ groupId: gid, tab: t, isActive: g.activeTabId === t.id });
      }
    }
    return entries;
  };

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
      {/* Keyed by group id + tab id: solid-dnd draggable data is a
          registration-time snapshot, so a layout restore (or a tab moving
          between leaf groups) must remount the row — otherwise every drag
          would carry a dead sourceGroupId and moveTab would silently no-op.
          Keying also survives updateTab replacing tab objects on every write
          (same trap as TabStrip). */}
      <Key each={leafEntries()} by={(e) => `${e.groupId}:${e.tab.id}`}>
        {(entry) => (
          <RibbonTabButton
            position={props.position}
            tab={entry().tab}
            groupId={entry().groupId}
            isActive={entry().isActive}
            isVertical={isVertical()}
          />
        )}
      </Key>
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
  const isCollapsed = () => panel().isCollapsed;
  const isVertical = () => props.position === 'left' || props.position === 'right';

  const expandedPanel = () => (
    <>
      {(props.position === 'right' || props.position === 'bottom') && (
        <EdgePanelResizeHandle position={props.position} />
      )}
      {/* No border here — the ribbon and handle lines are the separators.
          The panel body is a full layout tree rendered by the same
          SplitPane/Pane stack as the center tiling, so edge panels split,
          host tab bars, and accept drops exactly like center panes. */}
      <div
        class="flex flex-col overflow-hidden"
        style={
          isVertical()
            ? { width: panel().width ? `${panel().width}px` : '250px', 'min-width': '0' }
            : { height: panel().height ? `${panel().height}px` : '200px', 'min-height': '0' }
        }
      >
        <div class="flex-1 min-h-0 min-w-0">
          <SplitPane node={panel().layout} />
        </div>
      </div>
      {props.position === 'left' && <EdgePanelResizeHandle position={props.position} />}
    </>
  );

  // Slide without live reflow: the clip frame SNAPS between 0 and full size
  // (one layout change per toggle) while the inner panel — kept at its full
  // final size — slides via compositor-driven `translate`. Layout-affecting
  // properties are never transitioned: tweening width/height re-lays-out the
  // neighboring content every frame AND runs on the main thread out of sync
  // with the compositor's translate, which reads as jitter. On open the
  // frame reserves the space first and the panel slides into it; on close
  // the panel slides out, then the frame collapses.
  const TWEEN_MS = 200;
  const [rendered, setRendered] = createSignal(!isCollapsed());
  const [open, setOpen] = createSignal(!isCollapsed());
  let tweenTimer: number | undefined;

  createEffect(
    on(
      isCollapsed,
      (collapsed) => {
        window.clearTimeout(tweenTimer);
        if (!collapsed) {
          setRendered(true);
          // Double rAF: paint the offscreen state first, THEN flip so the
          // browser has something to transition from.
          requestAnimationFrame(() => requestAnimationFrame(() => setOpen(true)));
        } else {
          setOpen(false);
          tweenTimer = window.setTimeout(() => setRendered(false), TWEEN_MS + 50);
        }
      },
      { defer: true },
    ),
  );
  onCleanup(() => window.clearTimeout(tweenTimer));

  // Panel size + the 1px resize handle that lives inside the wrapper.
  const fullSize = () => (isVertical() ? (panel().width || 250) : (panel().height || 200)) + 1;
  const offscreen = () =>
    props.position === 'left' ? '-100% 0' : props.position === 'right' ? '100% 0' : '0 100%';
  const frameStyle = () => ({
    [isVertical() ? 'width' : 'height']: `${fullSize()}px`,
  });
  const innerStyle = () => ({
    [isVertical() ? 'width' : 'height']: `${fullSize()}px`,
    translate: open() ? '0 0' : offscreen(),
    transition: `translate ${TWEEN_MS}ms ease-out`,
  });

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
      <Show when={rendered()}>
        {/* Clip frame: fixed at full size while mounted, so neighboring
            content reflows exactly once per toggle… */}
        <div
          classList={{
            'flex overflow-hidden flex-none': true,
            'flex-row': isVertical(),
            'flex-col': !isVertical(),
          }}
          style={frameStyle()}
        >
          {/* …while the panel itself slides within it, at full opacity. */}
          <div
            classList={{
              'flex flex-none': true,
              'flex-row': isVertical(),
              'flex-col': !isVertical(),
            }}
            style={innerStyle()}
          >
            {expandedPanel()}
          </div>
        </div>
      </Show>
      {props.position !== 'left' && <EdgeRibbon position={props.position} />}
    </div>
  );
};
