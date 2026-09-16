import { Component, Show, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore, windowActions, policy } from '@/windowing/store';
import { collectPanes, findPaneInLayout, primaryEdgeGroupId } from '@/windowing/model/tree';
import type { EdgePanelPosition, Tab } from '@/windowing/model/types';
import { isEdgeCollapsed } from '@/windowing/model/types';
import { useWindowing } from '@/windowing/components/context';
import { RibbonPaneStrip } from './RibbonPaneStrip';
import {
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
    { type: 'tab', tab: props.tab, sourceGroupId: props.groupId },
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
    !isEdgeCollapsed(windowStore.edgePanels[props.position]) &&
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
      title={unavailable() ? `${props.tab.title} — ${reason()}` : props.tab.title}
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

/** The look of every ribbon button. The app's rail chrome uses it too. */
export const ribbonBtn =
  'flex items-center justify-center text-muted-dark hover:text-shell-body hover:bg-hover-wash transition-colors';

/** One command button on the ribbon (opens a modal/panel — Obsidian puts
 * these on the ribbon: palette, quick actions, settings gear at bottom). */
export const RibbonCommand: Component<{
  title: string;
  testId: string;
  onClick: () => void;
  children: ReturnType<Component>;
}> = (props) => (
  <button
    type="button"
    data-testid={props.testId}
    class={`${ribbonBtn} w-10 h-9 flex-none`}
    title={props.title}
    onClick={() => props.onClick()}
  >
    {props.children}
  </button>
);

/** The always-visible icon bar at the window edge (Obsidian's ribbon):
 * panels grow out of it, so the toggles never move or disappear. The top
 * button expands/collapses the panel. */
export const Ribbon: Component<{ position: EdgePanelPosition }> = (props) => {
  const { slots } = useWindowing();
  // The box the pane markers are positioned inside. They are placed from the
  // PANEL's measured geometry, so they need this element's own top to convert
  // a viewport coordinate into an offset.
  let ribbonRef: HTMLElement | undefined;
  const panel = () => windowStore.edgePanels[props.position];

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
    const cleanup = slots.attachDropTarget?.(el, () =>
      primaryEdgeGroupId(windowStore, props.position),
    );
    onCleanup(() => cleanup?.());
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
      classList={{
        'relative flex flex-col bg-shell-bg border-hairline transition-colors data-file-drop-over:bg-primary/20': true,
        // Border faces the center/panel it grows toward.
        'border-r': props.position === 'left',
        'border-l': props.position === 'right',
        'bg-primary/20': droppable.isActiveDroppable,
      }}
    >
      {/* Top slot: this bar's panel toggle — always in view. */}
      <button
        type="button"
        data-testid={`ribbon-toggle-${props.position}`}
        data-ribbon-ceiling
        // z-20: the topmost pane's own top edge is y=0, the same 36px this
        // button occupies. The rail toggle owns those pixels, so the overlay
        // never draws a marker there — see RibbonPaneStrip's floor.
        class={`${ribbonBtn} flex-none relative z-20 bg-shell-bg w-10 h-9 border-b border-hairline`}
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
      {/* The pane markers are an OVERLAY, not a row in this flow: each one is
          placed at the top edge of the pane it controls, measured off the
          panel beside it. In the flow they inherited the offset of every fixed
          cluster above them and pointed at the wrong pane. */}
      <Show when={panes().length > 1}>
        <RibbonPaneStrip position={props.position} ribbonEl={() => ribbonRef} />
      </Show>
      {slots.railHead?.(props.position)}
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
                isVertical
              />
            )}
          </Key>
        </div>
      </Show>
      {/* The tail: swapping sides on the left rail, then the app's tail slot.
          The window manager, not the slot, claims the floor for this run when
          no trailing tab cluster above claims it, because only the window
          manager knows whether that cluster exists. z-20 keeps the pane
          markers under it. */}
      <div
        class="flex flex-none flex-col"
        classList={{ 'relative z-20 bg-shell-bg mt-auto': trailingEntries().length === 0 }}
        data-ribbon-floor={trailingEntries().length === 0 ? '' : undefined}
      >
        <Show when={props.position === 'left'}>
          {/* Swapping sides acts on the whole shell, so it sits at the bottom
              of the rail with the other shell-wide toggles. */}
          <RibbonCommand
            title="Swap side panels (Ctrl+Shift+\)"
            testId="ribbon-cmd-swap-sides"
            onClick={() => windowActions.swapSidePanels()}
          >
            <ArrowLeftRight class="w-4 h-4" />
          </RibbonCommand>
        </Show>
        {slots.railTail?.(props.position)}
      </div>
    </div>
  );
};
