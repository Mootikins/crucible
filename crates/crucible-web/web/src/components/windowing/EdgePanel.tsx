import { Component, Show, createEffect, createSignal, on, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions } from '@/stores/windowStore';
import { LayoutMenu } from '@/components/shell/LayoutMenu';
import { OfflineBadge } from '@/components/OfflineBadge';
import { applyTheme, theme } from '@/lib/theme';
import {
  collectPanes,
  findPaneInLayout,
  primaryEdgeGroupId,
} from '@/stores/windowStoreInternals';
import type { EdgePanelPosition, Tab } from '@/types/windowTypes';
import { isRestoringLayout } from '@/lib/layout-restore';
import { attachFileDropTarget } from '@/lib/file-dnd';
import { openFileInGroup } from '@/lib/file-actions';
import { terminalAllowed } from '@/lib/terminal-availability';
import { SplitPane } from './SplitPane';
import { RibbonPaneStrip } from './RibbonPaneStrip';
import {
  IconPanelLeft,
  IconPanelLeftClose,
  IconPanelRight,
  IconPanelRightClose,
  IconPanelBottom,
  IconPanelBottomClose,
  IconSettings,
  IconMoon,
  IconSun,
  IconBell,
} from './icons';
import { ArrowLeftRight } from '@/lib/icons';
import { notificationStore } from '@/stores/notificationStore';
import { NotificationCenter } from '@/components/NotificationCenter';

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
  paneId: string;
  isActive: boolean;
  isVertical: boolean;
}> = (props) => {
  const draggable = createDraggable(
    `edgetab-collapsed:${props.position}:${props.tab.id}`,
    { type: 'tab', tab: props.tab, sourceGroupId: props.groupId },
  );

  // A terminal this client can't use (remote without the remote_shell
  // opt-in) is greyed out instead of opening a panel that only explains
  // why it won't connect. Still draggable — moving the tab is harmless.
  const unavailable = () =>
    props.tab.contentType === 'terminal' && !terminalAllowed();

  const paneCollapsed = () =>
    findPaneInLayout(windowStore.edgePanels[props.position].layout, props.paneId)
      ?.collapsed === true;

  const handleClick = () => {
    if (unavailable()) return;
    const panel = windowStore.edgePanels[props.position];
    if (panel.isCollapsed) {
      windowActions.setActiveTab(props.groupId, props.tab.id);
      windowActions.setEdgePanelCollapsed(props.position, false);
    } else if (paneCollapsed()) {
      // The rail is open and this tab's PANE is the thing tucked away, so
      // open the pane. Collapsing the whole rail here would hide the tabs the
      // user can plainly see.
      windowActions.setActiveTab(props.groupId, props.tab.id);
      windowActions.setPaneCollapsed(props.paneId, false);
    } else if (props.isActive) {
      windowActions.setEdgePanelCollapsed(props.position, true);
    } else {
      windowActions.setActiveTab(props.groupId, props.tab.id);
    }
  };

  const highlighted = () =>
    props.isActive &&
    !windowStore.edgePanels[props.position].isCollapsed &&
    !paneCollapsed();

  return (
    <button
      use:draggable
      type="button"
      data-testid={`collapsed-tab-button-${props.position}`}
      classList={{
        'flex items-center justify-center transition-all duration-150': true,
        'w-10 h-10': props.isVertical,
        'h-9 px-3': !props.isVertical,
        'opacity-40': draggable.isActiveDraggable || unavailable(),
        'cursor-not-allowed': unavailable(),
        'bg-surface-elevated text-shell-ink':
          highlighted() && !draggable.isActiveDraggable && !unavailable(),
        'text-muted-dark hover:text-shell-body hover:bg-hover-wash':
          !highlighted() && !draggable.isActiveDraggable && !unavailable(),
        'text-muted-dark': unavailable() && !draggable.isActiveDraggable,
      }}
      title={
        unavailable()
          ? `${props.tab.title} — only available from the host machine (or with remote_shell enabled)`
          : props.tab.title
      }
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

/**
 * The notification bell, pinned to the bottom of the right ribbon.
 *
 * Its own component rather than a `RibbonCommand` because it carries an unread
 * badge and owns a popout, neither of which that shape supports. Keeps
 * `data-testid="corner-bell"` from its previous home so existing locators
 * resolve.
 */
const RibbonBell: Component<{
  /**
   * Claim the rail's free space.
   *
   * False when a trailing tab cluster above already claims it — two `mt-auto`
   * siblings split the gap between them and both end up mid-rail.
   */
  pinBottom: boolean;
}> = (props) => {
  const [open, setOpen] = createSignal(false);
  const unreadCount = () => notificationStore.notificationCount();
  let bellRef: HTMLButtonElement | undefined;

  return (
    <>
      <button
        type="button"
        ref={bellRef}
        data-testid="corner-bell"
        data-ribbon-floor={props.pinBottom ? '' : undefined}
        class={`${ribbonBtn} relative z-20 bg-shell-bg w-10 h-9 flex-none`}
        classList={{ 'text-shell-body': open(), 'mt-auto': props.pinBottom }}
        title="Notifications"
        aria-label="Toggle notifications"
        onClick={() => setOpen(!open())}
      >
        <IconBell class="w-4 h-4" />
        <Show when={unreadCount() > 0}>
          {/* The count was 8px — three steps under the app's 11px floor, and
              unreadable at a glance, which is the badge's only job. It reads
              the floor now, and the badge grew to hold it: a 15px pill that
              still clears the 16px icon it sits on. `tabular-nums` keeps 1 and
              9 the same width, so the badge does not twitch as the count
              climbs. */}
          <span class="absolute -top-0.5 -right-0.5 px-1 min-w-[15px] text-center rounded-full bg-error text-white text-floor font-semibold leading-[15px] tabular-nums">
            {unreadCount() > 99 ? '99+' : unreadCount()}
          </span>
        </Show>
      </button>
      <NotificationCenter open={open()} onClose={() => setOpen(false)} anchor={bellRef} />
    </>
  );
};

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
    data-ribbon-floor={props.bottom ? '' : undefined}
    classList={{
      [`${ribbonBtn} w-10 h-9 flex-none`]: true,
      'relative z-20 bg-shell-bg mt-auto': !!props.bottom,
    }}
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
  // The box the pane markers are positioned inside. They are placed from the
  // PANEL's measured geometry, so they need this element's own top to convert
  // a viewport coordinate into an offset.
  let ribbonRef: HTMLElement | undefined;
  const panel = () => windowStore.edgePanels[props.position];
  const isVertical = () => props.position === 'left' || props.position === 'right';

  // The panel is a layout tree — the ribbon shows every tab across all leaf
  // groups, in tree order. Each leaf group keeps its own active tab (one
  // highlighted icon per split).
  const leafEntries = () => {
    const entries: { groupId: string; paneId: string; tab: Tab; isActive: boolean }[] = [];
    for (const pane of collectPanes(panel().layout)) {
      const g = pane.tabGroupId ? windowStore.tabGroups[pane.tabGroupId] : undefined;
      if (!g) continue;
      for (const t of g.tabs) {
        entries.push({
          groupId: g.id,
          paneId: pane.id,
          tab: t,
          isActive: g.activeTabId === t.id,
        });
      }
    }
    return entries;
  };

  /**
   * The pane ids in the panel's TRAILING branch — the bottom half of a
   * vertical rail, the right half of a horizontal one.
   *
   * A ribbon button opens a pane, so it belongs on the same half of the rail
   * as the pane it opens. Every leaf button used to render in one run from the
   * top, so the terminal — which lives in the bottom pane of the right panel —
   * sat at the top of the rail, as far from its own pane as the rail allows.
   * You had to cross the whole edge to reach the thing beside you.
   *
   * Only the ROOT split is consulted. Deeper nesting is a rare shape, and
   * mapping every nesting level onto rail thirds would produce clusters the
   * user cannot predict; "top half or bottom half" is a rule you can see.
   */
  const trailingPaneIds = () => {
    const root = panel().layout;
    if (root.type !== 'split') return new Set<string>();
    return new Set(collectPanes(root.second).map((p) => p.id));
  };
  const leadingEntries = () => leafEntries().filter((e) => !trailingPaneIds().has(e.paneId));
  const trailingEntries = () => leafEntries().filter((e) => trailingPaneIds().has(e.paneId));

  // Per-PANE markers only earn their space once a rail holds more than one
  // pane. With a single pane the rail's own toggle already is that control,
  // and a lone marker beside it would be two buttons for one thing.
  const panes = () => collectPanes(panel().layout);

  const droppable = createDroppable(`edgepanel-collapsed:${props.position}`, {
    type: 'edgePanel',
    panelId: props.position,
  });

  // Native file drags (pragmatic pipeline, separate from solid-dnd tab
  // drags): dropping a file on the ribbon opens it in this panel's first
  // leaf group and expands the panel — same affordance the tab drop has.
  const [fileDropOver, setFileDropOver] = createSignal(false);
  const attachRibbonFileDrop = (el: HTMLElement) => {
    const cleanup = attachFileDropTarget(el, {
      zone: 'ribbon',
      canDrop: (source) => !source.isDir,
      onDragEnter: () => setFileDropOver(true),
      onDragLeave: () => setFileDropOver(false),
      onDrop: (source) => {
        setFileDropOver(false);
        const groupId = primaryEdgeGroupId(windowStore, props.position);
        if (!groupId) return;
        openFileInGroup(groupId, source.absPath, source.name);
        windowActions.setEdgePanelCollapsed(props.position, false);
      },
    });
    onCleanup(cleanup);
  };

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
      ref={(el) => {
        ribbonRef = el;
        attachRibbonFileDrop(el);
      }}
      data-testid={`edge-collapsed-drop-${props.position}`}
      classList={{
        'relative flex bg-shell-bg border-hairline transition-colors': true,
        // Border faces the center/panel it grows toward.
        'flex-col border-r': props.position === 'left',
        'flex-col border-l': props.position === 'right',
        'flex-row border-t': !isVertical(),
        'bg-primary/20': droppable.isActiveDroppable || fileDropOver(),
      }}
    >
      {/* Top/leading slot: this bar's panel toggle — always in view. */}
      <button
        type="button"
        data-testid={`ribbon-toggle-${props.position}`}
        data-ribbon-ceiling
        classList={{
          [`${ribbonBtn} flex-none`]: true,
          // z-20: the topmost pane's own top edge is y=0, the same 36px this
          // button occupies. The rail toggle owns those pixels, so the overlay
          // never draws a marker there — see RibbonPaneStrip's floor.
          'relative z-20 bg-shell-bg w-10 h-9 border-b border-hairline': isVertical(),
          'h-9 px-2 border-r border-hairline': !isVertical(),
        }}
        title={panel().isCollapsed ? 'Expand panel' : 'Collapse panel'}
        onClick={() => windowActions.toggleEdgePanel(props.position)}
      >
        {toggleIcon()}
      </button>
      {/* The top of the left rail used to carry a command-palette bolt and a
          new-session plus. Both are gone. Neither was the fastest route to its
          own action — the palette is Ctrl+P and is itself a list of every
          command, and a new session is one hover-click in the session tree,
          seeded correctly — so each was a third doorway competing for the most
          reachable pixels on the rail with the tab buttons that have no other
          doorway at all. Swapping sides moved to the bottom cluster below,
          beside the other two shell-wide toggles. */}
      {/* Keyed by group id + tab id: solid-dnd draggable data is a
          registration-time snapshot, so a layout restore (or a tab moving
          between leaf groups) must remount the row — otherwise every drag
          would carry a dead sourceGroupId and moveTab would silently no-op.
          Keying also survives updateTab replacing tab objects on every write
          (same trap as TabStrip). */}
      <Key each={leadingEntries()} by={(e) => `${e.groupId}:${e.tab.id}`}>
        {(entry) => (
          <RibbonTabButton
            position={props.position}
            tab={entry().tab}
            groupId={entry().groupId}
            paneId={entry().paneId}
            isActive={entry().isActive}
            isVertical={isVertical()}
          />
        )}
      </Key>
      {/* The pane markers are an OVERLAY, not a row in this flow: each one is
          placed at the top edge of the pane it controls, measured off the
          panel beside it. In the flow they inherited the offset of every fixed
          cluster above them and pointed at the wrong pane. */}
      <Show when={panes().length > 1}>
        <RibbonPaneStrip position={props.position} ribbonEl={() => ribbonRef} />
      </Show>
      <Show when={props.position === 'left'}>
        {/* Layout actions — put a closed pane back, or start the layout
            over. On the rail because the rail IS the layout: the two repairs
            belong on the thing they repair, not on a settings page the user
            has to find while looking at what they broke.

            Project actions used to sit here. They moved into the sessions
            pane, beside the projects they act on. */}
        <div class="flex-none w-10 h-9 flex items-center justify-center border-b border-hairline">
          <LayoutMenu />
        </div>
      </Show>
      {/* Everything from here down is pinned to the rail's far end.
          EXACTLY ONE element in this run may carry `mt-auto` — it is what
          absorbs the free space — and it must be the FIRST of them, or the
          space splits between claimants and the cluster floats mid-rail.
          `data-ribbon-floor` marks that same element for RibbonPaneStrip,
          which bounds its overlay between the ceiling and the floor. */}
      <Show when={trailingEntries().length > 0}>
        <div class="mt-auto flex flex-none flex-col" data-ribbon-floor>
          <Key each={trailingEntries()} by={(e) => `${e.groupId}:${e.tab.id}`}>
            {(entry) => (
              <RibbonTabButton
                position={props.position}
                tab={entry().tab}
                groupId={entry().groupId}
                paneId={entry().paneId}
                isActive={entry().isActive}
                isVertical={isVertical()}
              />
            )}
          </Key>
        </div>
      </Show>
      <Show when={props.position === 'left'}>
        {/* Swapping sides, the theme and settings: three shell-wide toggles,
            together at the bottom-left like Obsidian's gear. Swap joined them
            from the rail's top, where it sat among per-panel controls while
            acting on the whole shell. */}
        <RibbonCommand
          title="Swap side panels (Ctrl+Shift+\)"
          testId="ribbon-cmd-swap-sides"
          bottom={trailingEntries().length === 0}
          onClick={() => windowActions.swapSidePanels()}
        >
          <ArrowLeftRight class="w-4 h-4" />
        </RibbonCommand>
        <OfflineBadge />
        <RibbonCommand
          title={theme() === 'light' ? 'Switch to dark theme' : 'Switch to light theme'}
          testId="ribbon-cmd-theme"
          onClick={() => {
            applyTheme(theme() === 'light' ? 'dark' : 'light');
          }}
        >
          <Show when={theme() === 'light'} fallback={<IconSun class="w-4 h-4" />}>
            <IconMoon class="w-4 h-4" />
          </Show>
        </RibbonCommand>
        <RibbonCommand
          title="Settings"
          testId="ribbon-cmd-settings"
          // A dialog, not a tab. Changing a setting is a detour you return
          // from; it never wanted a pane, a split or a place in the layout.
          onClick={() => window.dispatchEvent(new CustomEvent('crucible:open-settings'))}
        >
          <IconSettings class="w-4 h-4" />
        </RibbonCommand>
      </Show>
      {/* The bell mirrors the left ribbon's gear: pinned to the bottom of the
          RIGHT rail. It used to float in the centre pane's bottom-right corner,
          where it sat over the document and vanished with the rest of the
          transient chip cluster. The rail is rendered outside the slide clip
          frame, so this survives collapsing the panel. */}
      <Show when={props.position === 'right'}>
        <RibbonBell pinBottom={trailingEntries().length === 0} />
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
      {props.position === 'right' && (
        <EdgePanelResizeHandle position={props.position} />
      )}
      {/* No border here — the ribbon and handle lines are the separators.
          The panel body is a full layout tree rendered by the same
          SplitPane/Pane stack as the center tiling, so edge panels split,
          host tab bars, and accept drops exactly like center panes. */}
      <div
        data-edge-panel-body={props.position}
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

  // Slide with SYNCHRONIZED reflow, no remount: the panel content stays
  // MOUNTED while collapsed — clipped to zero size and visibility:hidden —
  // so a toggle never re-mounts the panel subtree (the old mount-on-expand
  // lifecycle spent ~500ms building the file tree exactly when the slide
  // should start; staying mounted also preserves tree expansion and
  // terminal scrollback). One rAF loop drives BOTH the clip frame's size
  // and the inner panel's translate from a single progress value, so the
  // neighboring content reflows smoothly across the whole toggle and the
  // clip edge stays pixel-locked to the panel edge. CSS transitions are
  // deliberately NOT used: width/height transitions run on the main thread
  // while `translate` runs on the compositor, and under load the two
  // desync — the panel visibly tears against its own clip edge.
  const TWEEN_MS = 200;

  /**
   * Whether the viewer asked for less motion.
   *
   * `matchMedia` and not a CSS variable: this tween runs in JavaScript, so the
   * media query has to be read rather than cascaded. Guarded because jsdom
   * (and any environment without `matchMedia`) must fall through to animating
   * rather than throw at panel construction.
   */
  const prefersReducedMotion = () =>
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const [progress, setProgress] = createSignal(isCollapsed() ? 0 : 1);
  let tweenRaf: number | undefined;

  createEffect(
    on(
      isCollapsed,
      (collapsed) => {
        const target = collapsed ? 0 : 1;
        if (tweenRaf !== undefined) cancelAnimationFrame(tweenRaf);
        const from = progress();
        if (from === target) return;
        // A layout RESTORE snaps: it's initialization, not an interaction —
        // tweening on page load looks wrong and slides the center layout
        // under anything that just measured it (stale-coordinate drags).
        //
        // So does a REDUCED-MOTION preference. index.css zeroes every
        // animation and transition under `prefers-reduced-motion`, and this is
        // the one animation in the app deliberately moved OUT of CSS — which
        // silently opted it out of that promise. A user who asked for no motion
        // still got a 200ms slide across a third of the window, which is the
        // largest moving thing the shell draws.
        //
        // Queried here rather than cached at module load so a preference
        // changed mid-session takes effect on the next toggle.
        if (isRestoringLayout() || prefersReducedMotion()) {
          setProgress(target);
          return;
        }
        // Duration scales with remaining distance so a mid-flight reversal
        // doesn't crawl.
        const dur = Math.max(1, TWEEN_MS * Math.abs(target - from));
        const start = performance.now();
        const step = (now: number) => {
          const t = Math.min(1, (now - start) / dur);
          const eased = 1 - (1 - t) * (1 - t); // ease-out
          setProgress(from + (target - from) * eased);
          tweenRaf = t < 1 ? requestAnimationFrame(step) : undefined;
        };
        tweenRaf = requestAnimationFrame(step);
      },
      { defer: true },
    ),
  );
  onCleanup(() => {
    if (tweenRaf !== undefined) cancelAnimationFrame(tweenRaf);
  });

  // Panel size + the 1px resize handle that lives inside the wrapper.
  const fullSize = () => (isVertical() ? (panel().width || 250) : (panel().height || 200)) + 1;
  const frameStyle = () => ({
    [isVertical() ? 'width' : 'height']: `${Math.round(fullSize() * progress())}px`,
    // Fully closed panels leave paint, hit-testing, and the tab order —
    // clipped-but-visible content is still keyboard-reachable otherwise.
    visibility: progress() > 0 ? ('visible' as const) : ('hidden' as const),
  });
  const innerStyle = () => {
    const off = (1 - progress()) * 100;
    const translate =
      props.position === 'left'
        ? `${-off}% 0`
        : props.position === 'right'
          ? `${off}% 0`
          : `0 ${off}%`;
    return {
      [isVertical() ? 'width' : 'height']: `${fullSize()}px`,
      translate,
    };
  };

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
      {/* Clip frame: snaps 0 ↔ full size (one reflow per toggle), content
          always mounted… */}
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
      {props.position !== 'left' && <EdgeRibbon position={props.position} />}
    </div>
  );
};
