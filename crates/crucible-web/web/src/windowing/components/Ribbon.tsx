import { Component, Show, createSignal, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import { createDraggable, createDroppable, useDragDropContext } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions, policy } from '@/windowing/store';
import { collectPanes, findPaneInLayout, primaryEdgeGroupId } from '@/windowing/model/tree';
import type { EdgePanelPosition, Tab } from '@/windowing/model/types';
import { isEdgeCollapsed } from '@/windowing/model/types';
import { useWindowing } from '@/windowing/components/context';
import { chordLabel } from '@/windowing/shortcuts';
import { RibbonPaneStrip } from './RibbonPaneStrip';
import { RibbonCommand, ribbonBtn } from './RibbonButton';
import { TabContextMenu, useTabBarDnD } from './TabBar';
import { paneTopIn, railBodyEl, watchRailGeometry } from './rail-geometry';
import { railShown } from './rail-shown';
import { confirmTabClose } from '@/windowing/model/tab-guards';
import {
  IconClose,
  IconPanelLeft,
  IconPanelLeftClose,
  IconPanelRight,
  IconPanelRightClose,
} from './icons';
import { ArrowLeftRight } from '@/lib/icons';

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
    { type: 'tab', tab: props.tab, sourceGroupId: props.groupId, ribbonSide: props.position },
  );

  // A tab that the policy marks unavailable is greyed out instead of opening
  // a panel that only explains why it cannot work. Still draggable — moving
  // the tab is harmless.
  const reason = () => policy().unavailableReason(props.tab);
  const unavailable = () => reason() !== null;

  const paneCollapsed = () =>
    findPaneInLayout(windowStore.edgePanels[props.position].layout, props.paneId)
      ?.collapsed === true;

  const handleClick = () => {
    if (unavailable()) return;
    const panel = windowStore.edgePanels[props.position];
    if (isEdgeCollapsed(panel)) {
      windowActions.setActiveTab(props.groupId, props.tab.id);
      windowActions.setEdgePanelCollapsed(props.position, false);
      windowActions.setPaneCollapsed(props.paneId, false);
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

  // The icon shows its pane as open while any of the rail is on screen, so a
  // theme's highlight appears when the rail starts to open and leaves when it
  // has slid shut (see rail-shown.ts).
  const highlighted = () => props.isActive && railShown(props.position) && !paneCollapsed();

  // The state rides on data attributes. The theme decides how each one looks.
  // The tab menu of the tab bar wraps the icon: a theme can hide the tab bars
  // of a rail, and then this icon is the only handle of the tab. For the same
  // reason the icon carries its own close control; the theme shows it on
  // hover and on focus. A tab that the policy keeps gets none.
  const closable = () => windowActions.canCloseTab(props.groupId, props.tab.id);
  return (
    <TabContextMenu groupId={() => props.groupId} paneId={() => props.paneId} tab={props.tab}>
    <div class="wm-ribbon-tab-slot relative flex-none" data-testid={`rail-tab-${props.tab.id}`}>
    <button
      use:draggable
      type="button"
      data-testid={`collapsed-tab-button-${props.position}`}
      data-ribbon-tab-id={props.tab.id}
      data-group-id={props.groupId}
      data-content-type={props.tab.contentType}
      data-orientation={props.isVertical ? 'vertical' : 'horizontal'}
      data-highlighted={highlighted() ? '' : undefined}
      data-unavailable={unavailable() ? '' : undefined}
      data-dragging={draggable.isActiveDraggable ? '' : undefined}
      classList={{
        'wm-ribbon-tab flex items-center justify-center': true,
        // The column width is part of RIBBON_WIDTH_PX.
        'w-10': props.isVertical,
        'cursor-not-allowed': unavailable(),
      }}
      title={unavailable() ? `${props.tab.title} — ${reason()}` : props.tab.title}
      onClick={handleClick}
    >
      {props.tab.icon ? (
        <props.tab.icon class="w-4 h-4" />
      ) : (
        <span class="wm-icon-letter truncate max-w-[2rem]">{props.tab.title[0]}</span>
      )}
    </button>
    <Show when={closable()}>
      <button
        type="button"
        aria-label={`Close ${props.tab.title}`}
        data-testid={`ribbon-tab-close-${props.position}`}
        class="wm-ribbon-tab-close absolute top-1 right-1 flex h-3.5 w-3.5 items-center justify-center"
        onClick={(e) => {
          e.stopPropagation();
          if (confirmTabClose(props.tab)) windowActions.removeTab(props.groupId, props.tab.id);
        }}
      >
        <IconClose class="w-2.5 h-2.5" />
      </button>
    </Show>
    </div>
    </TabContextMenu>
  );
};

/**
 * The ribbon's width: a `w-10` button column and its 1px border. With the
 * ribbon inside the rail, it is the width a closed rail keeps.
 *
 * This is a layout contract. The component sets the column width and the
 * border width, and the slide code reads this number. A theme may change the
 * border colour. A theme must not change the column width or the border width.
 */
export const RIBBON_WIDTH_PX = 41;

/** The always-visible icon bar at the window edge (Obsidian's ribbon):
 * panels grow out of it, so the toggles never move or disappear. The top
 * button expands/collapses the panel. */
export const Ribbon: Component<{ position: EdgePanelPosition }> = (props) => {
  const windowing = useWindowing();
  // The box the pane markers are positioned inside. They are placed from the
  // PANEL's measured geometry, so they need this element's own top to convert
  // a viewport coordinate into an offset.
  let ribbonRef: HTMLElement | undefined;
  const panel = () => windowStore.edgePanels[props.position];
  const dnd = useDragDropContext();
  const { insertOffset } = useTabBarDnD({
    groupId: () => {
      const source = dnd?.[0].active.draggable?.data;
      return source?.type === 'tab' && collectPanes(panel().layout).some(p => p.tabGroupId === source.sourceGroupId)
        ? source.sourceGroupId as string : '';
    },
    tabsContainerRef: () => ribbonRef,
    axis: 'y',
  });
  const insideRail = () => windowStore.ribbonPlacement === 'panel';

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
   * The pane ids in the panel's TRAILING branch — the bottom half of the rail.
   *
   * A ribbon button opens a pane, so it belongs on the same half of the rail
   * as the pane it opens. Every leaf button used to render in one run from the
   * top, so a tool in the bottom pane of a rail sat at the top of the rail, as
   * far from its own pane as the rail allows.
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

  // ── The geometry that the ribbon publishes for a theme ──────────────────
  //
  // The trailing cluster sits at the far end of the ribbon. A theme can put
  // it at the top edge of its own pane instead. That position is a look, so
  // the theme owns it. The component measures the geometry and publishes it
  // as custom properties on the ribbon, in px from the top of the ribbon:
  //
  // - `--wm-trailing-pane-top`: the top edge of the first pane of the
  //   trailing branch. It follows the split during a drag.
  // - `--wm-ribbon-ceiling`: the bottom of the leading run (the toggle, the
  //   leading tabs and the head slot).
  // - `--wm-ribbon-floor`: the top of the pinned tail.
  // - `--wm-ribbon-trailing-height`: the height of the trailing cluster.
  //
  // A property is absent when its element is absent.
  let leadingRef: HTMLDivElement | undefined;
  let tailRef: HTMLDivElement | undefined;
  const [trailingRef, setTrailingRef] = createSignal<HTMLElement>();
  const [geometry, setGeometry] = createSignal<Record<string, string>>({});

  /** The first pane of the trailing branch, or null for a single-pane rail. */
  const trailingPaneId = () => {
    const root = panel().layout;
    return root.type === 'split' ? (collectPanes(root.second)[0]?.id ?? null) : null;
  };

  const measureGeometry = () => {
    const ribbon = ribbonRef;
    if (!ribbon) return;
    const originTop = ribbon.getBoundingClientRect().top;
    const next: Record<string, string> = {};
    const px = (n: number) => `${n}px`;

    const body = railBodyEl(props.position);
    const paneId = trailingPaneId();
    const paneTop = body && paneId ? paneTopIn(body, paneId, originTop) : null;
    if (paneTop !== null) next['--wm-trailing-pane-top'] = px(paneTop);

    // The leading run is every flow child before the trailing cluster and the
    // tail. The pane strip is an overlay, not a part of the run.
    let ceiling: number | null = null;
    for (const child of Array.from(ribbon.children)) {
      if (child === trailingRef() || child === tailRef) break;
      if (child.hasAttribute('data-ribbon-overlay')) continue;
      const bottom = child.getBoundingClientRect().bottom - originTop;
      ceiling = ceiling === null ? bottom : Math.max(ceiling, bottom);
    }
    if (ceiling !== null) next['--wm-ribbon-ceiling'] = px(ceiling);
    if (tailRef) next['--wm-ribbon-floor'] = px(tailRef.getBoundingClientRect().top - originTop);
    const trailing = trailingRef();
    if (trailing) next['--wm-ribbon-trailing-height'] = px(trailing.getBoundingClientRect().height);
    setGeometry(next);
  };

  watchRailGeometry(props.position, measureGeometry, {
    extra: () => [leadingRef, trailingRef(), tailRef],
    // A tab that comes or goes changes the size of a cluster.
    key: () => `${leadingEntries().length}:${trailingEntries().length}`,
  });

  // Per-PANE markers only earn their space once a rail holds more than one
  // pane. With a single pane the rail's own toggle already is that control,
  // and a lone marker beside it would be two buttons for one thing.
  const panes = () => collectPanes(panel().layout);

  const droppable = createDroppable(`edgepanel-collapsed:${props.position}`, {
    type: 'edgePanel',
    panelId: props.position,
  });

  // Native drags from outside the window manager: the app's drop target opens
  // the dropped item in this rail's first leaf group.
  const attachRibbonDrop = (el: HTMLElement) => {
    const cleanup = windowing.slots.attachDropTarget?.(el, () =>
      primaryEdgeGroupId(windowStore, props.position),
    );
    onCleanup(() => cleanup?.());
  };

  const swapTitle = () => {
    const chord = chordLabel('swapSidePanels', policy().shortcuts);
    return chord === null ? 'Swap side panels' : `Swap side panels (${chord})`;
  };

  const toggleIcon = () => {
    const collapsed = isEdgeCollapsed(panel());
    return props.position === 'left'
      ? collapsed ? <IconPanelLeft class="w-4 h-4" /> : <IconPanelLeftClose class="w-4 h-4" />
      : collapsed ? <IconPanelRight class="w-4 h-4" /> : <IconPanelRightClose class="w-4 h-4" />;
  };

  return (
    <div
      use:droppable
      ref={(el) => {
        ribbonRef = el;
        attachRibbonDrop(el);
      }}
      data-testid={`edge-collapsed-drop-${props.position}`}
      // `data-drop-over` is DROP_OVER_ATTR (context.tsx): the app sets it for
      // a native drag. `data-drop-active` marks a tab drag over the ribbon.
      data-drop-active={droppable.isActiveDroppable ? '' : undefined}
      classList={{
        'wm-ribbon relative flex flex-col': true,
        // Border faces the body. At the window edge the body is on the
        // centre side; inside the rail it is on the window-edge side. The
        // border width is part of RIBBON_WIDTH_PX; the theme gives its colour.
        'border-r': (props.position === 'left') !== insideRail(),
        'border-l': (props.position === 'right') !== insideRail(),
      }}
      style={geometry()}
    >
      {/* Top slot: this bar's panel toggle — always in view. */}
      <button
        type="button"
        data-testid={`ribbon-toggle-${props.position}`}
        data-ribbon-ceiling
        // z-20: the topmost pane's own top edge is y=0, the same 36px this
        // button occupies. The rail toggle owns those pixels, so the overlay
        // never draws a marker there — see RibbonPaneStrip's floor.
        // h-9 is the tab bar height (COLLAPSED_PANE_PX), so the toggle lines up
        // with the tab bar of the topmost pane.
        class={`${ribbonBtn} wm-ribbon-toggle flex-none relative z-20 w-10 h-9`}
        title={isEdgeCollapsed(panel()) ? 'Expand panel' : 'Collapse panel'}
        onClick={() => windowActions.toggleEdgePanel(props.position)}
      >
        {toggleIcon()}
      </button>
      {/* Keyed by group id + tab id: solid-dnd draggable data is a
          registration-time snapshot, so a layout restore (or a tab moving
          between leaf groups) must remount the row — otherwise every drag
          would carry a dead sourceGroupId and moveTab would silently no-op.
          Keying also survives updateTab replacing tab objects on every write
          (same trap as TabStrip). */}
      {/* The leading run. The wrapper holds no look of its own: its part
          class gives a theme one name for the cluster. */}
      <div ref={leadingRef} class="wm-ribbon-leading flex flex-none flex-col">
        <Key each={leadingEntries()} by={(e) => `${e.groupId}:${e.tab.id}`}>
          {(entry) => (
            <RibbonTabButton
              position={props.position}
              tab={entry().tab}
              groupId={entry().groupId}
              paneId={entry().paneId}
              isActive={entry().isActive}
              isVertical
            />
          )}
        </Key>
      </div>
      {/* The pane markers are an OVERLAY, not a row in this flow: each one is
          placed at the top edge of the pane it controls, measured off the
          panel beside it. In the flow they inherited the offset of every fixed
          cluster above them and pointed at the wrong pane. */}
      <Show when={panes().length > 1}>
        <RibbonPaneStrip position={props.position} ribbonEl={() => ribbonRef} />
      </Show>
      {windowing.slots.railHead?.(props.position)}
      {/* Everything from here down is pinned to the rail's far end.
          EXACTLY ONE element in this run may carry `mt-auto` — it is what
          absorbs the free space — and it must be the FIRST of them, or the
          space splits between claimants and the cluster floats mid-rail.
          `data-ribbon-floor` marks that same element for RibbonPaneStrip,
          which bounds its overlay between the ceiling and the floor. */}
      <Show when={trailingEntries().length > 0}>
        {/* `wm-ribbon-trailing` names the cluster for a theme. A theme can
            place it at `--wm-trailing-pane-top`; see the geometry above. */}
        <div
          ref={setTrailingRef}
          class="wm-ribbon-trailing mt-auto flex flex-none flex-col"
          data-ribbon-floor
        >
          <Key each={trailingEntries()} by={(e) => `${e.groupId}:${e.tab.id}`}>
            {(entry) => (
              <RibbonTabButton
                position={props.position}
                tab={entry().tab}
                groupId={entry().groupId}
                paneId={entry().paneId}
                isActive={entry().isActive}
                isVertical
              />
            )}
          </Key>
        </div>
      </Show>
      <Show when={insertOffset() !== null}>
        <div class="wm-ribbon-insert" data-testid="rail-drop-indicator" style={{ top: `${insertOffset()}px` }} />
      </Show>
      {/* The tail: swapping sides on the left rail, then the app's tail slot.
          The window manager, not the slot, claims the floor for this run when
          no trailing tab cluster above claims it, because only the window
          manager knows whether that cluster exists. z-20 keeps the pane
          markers under it. */}
      <div
        ref={tailRef}
        class="wm-ribbon-tail flex flex-none flex-col"
        classList={{ 'relative z-20 mt-auto': trailingEntries().length === 0 }}
        data-ribbon-floor={trailingEntries().length === 0 ? '' : undefined}
      >
        <Show when={props.position === 'left' && !windowing.slots.railTail}>
          {/* The default tail is replaced by an application tail slot.
              Swapping sides acts on the whole window, so it sits at the bottom
              of the rail with the other window-wide toggles. */}
          <RibbonCommand
            title={swapTitle()}
            testId="ribbon-cmd-swap-sides"
            onClick={() => windowActions.swapSidePanels()}
          >
            <ArrowLeftRight class="w-4 h-4" />
          </RibbonCommand>
        </Show>
        {windowing.slots.railTail?.(props.position)}
      </div>
    </div>
  );
};
