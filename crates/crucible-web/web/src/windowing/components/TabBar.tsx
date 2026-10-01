import { Component, For, JSX, Show, createMemo, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import {
  createDraggable,
  createDroppable,
  useDragDropContext,
} from '@thisbeyond/solid-dnd';
import type { Tab as TabType, TabBarProps, DragSource } from '@/windowing/model/types';
import { windowStore, windowActions, findEdgePanelForGroup } from '@/windowing/store';
import { IconGripVertical, IconClose, IconPopOut } from './icons';
import { ChevronDown } from '@/lib/icons';
import { confirmTabClose } from '@/windowing/model/tab-guards';
import { menuContent, menuItem } from '@/components/ui/menu-style';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { attachNativeMenuGuard, tabsToClose, type TabMenuAction } from '@/windowing/context-menu';
import { WindowControls, useFloatingWindow } from './WindowControls';

// ── Tab titles ─────────────────────────────────────────────────────────

/** Characters a tab label keeps whole. Above this the middle goes. */
const TAB_TITLE_MAX = 28;
const TAB_TITLE_HEAD = 12;
const TAB_TITLE_TAIL = 8;

/**
 * Elide a long tab label from the MIDDLE.
 *
 * CSS truncates from the end, and the end is where a tab title carries its
 * meaning: `2026-09-15 Web UI Review.md` and `2026-09-15 Web UI Plan.md`
 * both read as `2026-09-15 W…` at the old 120px cap, so a strip of titles
 * with a shared prefix became a column of identical tabs. The head names the
 * item and the tail names what makes it different.
 *
 * The full label always rides on the element's `title`, so the elision costs
 * the user nothing but a hover.
 *
 * Splits by code point, not by UTF-16 unit — a title that opens with an emoji
 * must not be cut through the middle of one.
 */
export function elideTabTitle(title: string): string {
  const chars = [...title];
  if (chars.length <= TAB_TITLE_MAX) return title;
  return `${chars.slice(0, TAB_TITLE_HEAD).join('')}…${chars.slice(-TAB_TITLE_TAIL).join('')}`;
}

// ── Module-level reorder state (shared with WindowManager) ──────────────

export type ReorderState = {
  groupId: string;
  insertIndex: number;
} | null;

const [, setReorderState] = createSignal<ReorderState>(null);

// Non-reactive pending reorder state (survives reactive cleanup race)
let pendingReorder: ReorderState = null;
let pendingReorderOwner: symbol | null = null;

export function getPendingReorder(): ReorderState {
  return pendingReorder;
}

export function clearPendingReorder(): void {
  pendingReorder = null;
}

// ── Insert-index computation helper ─────────────────────────────────────

function computeInsertIndex(
  containerEl: HTMLElement,
  pointerX: number,
  draggedTabId?: string,
  axis: 'x' | 'y' = 'x',
  groupId?: string,
): { logical: number; display: number } | null {
  const tabEls = Array.from(containerEl.querySelectorAll<HTMLElement>('[data-tab-id], [data-ribbon-tab-id]')).filter(el => !groupId || el.dataset.groupId === groupId);
  let logicalIndex = 0;
  for (let i = 0; i < tabEls.length; i++) {
    const el = tabEls[i] as HTMLElement;
    if (draggedTabId && (el.dataset.tabId ?? el.dataset.ribbonTabId) === draggedTabId) continue;
    const rect = el.getBoundingClientRect();
    if (pointerX < (axis === 'x' ? rect.left + rect.width / 2 : rect.top + rect.height / 2)) return { logical: logicalIndex, display: i };
    logicalIndex++;
  }
  return { logical: logicalIndex, display: tabEls.length };
}

// ── Unified TabItem (replaces Tab + EdgeTab) ────────────────────────────

interface TabItemProps {
  tab: TabType;
  draggableId: string;
  draggableData: DragSource;
  isActive: boolean;
  isFocused: boolean;
  onClick: () => void;
  onClose: (e: MouseEvent) => void;
  /** False on a tab that the store refuses to close, because the policy
   * keeps it. The affordance goes with the capability: an X that does nothing
   * teaches the user that the app is broken. */
  closable?: boolean;
  testId?: string;
}

const TabItem: Component<TabItemProps> = (props) => {
  // Measured, not assumed: the fade exists for a title the box cuts off.
  const [titleRef, setTitleRef] = createSignal<HTMLSpanElement | undefined>();
  const [titleOverflows, setTitleOverflows] = createSignal(false);
  const measureTitle = () => {
    const el = titleRef();
    if (el) setTitleOverflows(el.scrollWidth > el.clientWidth + 1);
  };
  createEffect(() => {
    props.tab.title;
    props.isActive;
    const el = titleRef();
    if (!el) return;
    measureTitle();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measureTitle);
    ro.observe(el);
    onCleanup(() => ro.disconnect());
  });
  const draggable = createDraggable(props.draggableId, props.draggableData);
  const Icon = props.tab.icon;

  return (
    <div
      use:draggable
      data-tab-id={props.tab.id}
      // The app's content type, for a theme: for example, a theme can hide
      // the icon on a kind of tab that the title names well enough.
      data-content-type={props.tab.contentType}
      {...(props.testId ? { 'data-testid': props.testId } : {})}
      // The state rides on data attributes; the theme draws it. The default
      // theme follows Obsidian: the active tab is a raised chip, and the
      // focus of the region shows as ink weight and a hairline outline.
      data-active={props.isActive ? '' : undefined}
      data-focused={props.isFocused ? '' : undefined}
      data-modified={props.tab.isModified ? '' : undefined}
      data-dragging={draggable.isActiveDraggable ? '' : undefined}
      // `pr-5` reserves the width of the trailing slot in the tab's own box,
      // so the slot never covers a SHORT title. The slot is absolute and
      // costs no layout, so this padding is the only space that the close
      // control uses. It is a layout contract with the slot (`right-1 w-4`).
      class="wm-tab relative flex items-center pr-5 cursor-pointer"
      onClick={() => props.onClick()}
    >
      <div class="wm-tab-lead relative w-3.5 h-3.5 flex-shrink-0 cursor-grab active:cursor-grabbing">
        <Show when={Icon} fallback={
          <IconGripVertical class="wm-tab-grip w-3.5 h-3.5" />
        }>
          <>
            {Icon && <Icon class="wm-tab-icon absolute inset-0 w-3.5 h-3.5" />}
            <IconGripVertical class="wm-tab-grip absolute inset-0 w-3.5 h-3.5" />
          </>
        </Show>
      </div>
      {/* The title fades out under the trailing slot rather than being cut by
          it. The mask applies only while the title OVERFLOWS its box: a short
          title on an active tab used to fade its last letters for no reason,
          because the mask sat on the span whether or not anything was cut. */}
      <span
        ref={setTitleRef}
        class="wm-tab-title truncate max-w-(--cru-measure-tab)"
        data-overflows={titleOverflows() ? '' : undefined}
        title={elideTabTitle(props.tab.title) === props.tab.title ? undefined : props.tab.title}
      >
        {elideTabTitle(props.tab.title)}
      </span>

      {/* ONE trailing slot, out of the flow.
          The close button used to be a flex child, so every tab reserved ~20px
          for a glyph that is invisible on a resting tab — the title lost that
          width permanently, and tabs jumped as the active one changed. It now
          sits OVER the tab's trailing edge and takes no layout at all.

          The slot also carries the modified dot, and the two swap places:
          the dot marks unsaved work at rest, and hovering turns
          it into the control that discards it. Two separate marks would mean a
          dirty tab is the one tab you cannot close without aiming. */}
      <span class="pointer-events-none absolute right-1 top-1/2 flex h-4 w-4 -translate-y-1/2 items-center justify-center">
        <Show when={props.tab.isModified}>
          <span class="wm-tab-dot" data-testid="tab-modified-dot" />
        </Show>
        <Show when={props.closable !== false}>
        <button
          aria-label="Close tab"
          onClick={(e) => {
            e.stopPropagation();
            props.onClose(e);
          }}
          // The theme hides the control at rest on a tab that is not active,
          // or that holds a modified dot, and shows it on hover and focus.
          class="wm-tab-close pointer-events-auto absolute inset-0 flex items-center justify-center"
        >
          <IconClose class="w-3 h-3" />
        </button>
        </Show>
      </span>
    </div>
  );
};

// ── Insert indicator element ────────────────────────────────────────────

const InsertIndicator: Component = () => (
  <div class="wm-tab-insert flex-shrink-0 my-auto" />
);

interface UseTabBarDnDOptions {
  groupId: () => string;
  tabsContainerRef: () => HTMLElement | undefined;
  axis?: 'x' | 'y';
}

export function useTabBarDnD(options: UseTabBarDnDOptions) {
  const owner = Symbol();
  const [insertOffset, setInsertOffset] = createSignal<number | null>(null);
  const [insertIdx, setInsertIdx] = createSignal<number | null>(null);
  const dndCtx = useDragDropContext();

  const isSameBarDrag = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return false;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' && data.sourceGroupId === options.groupId();
  };

  const draggedTabId = () => {
    const active = dndCtx?.[0]?.active?.draggable;
    if (!active) return undefined;
    const data = active.data as DragSource | undefined;
    return data?.type === 'tab' ? data.tab.id : undefined;
  };

  createEffect(() => {
    const tabsContainerRef = options.tabsContainerRef();
    if (!isSameBarDrag() || !tabsContainerRef) {
      setInsertOffset(null);
      setInsertIdx(null);
      setReorderState(null);
      return;
    }
    const sensor = dndCtx?.[0]?.active?.sensor;
    const x = sensor?.coordinates?.current?.x;
    const y = sensor?.coordinates?.current?.y;
    if (x != null && y != null) {
      const rect = tabsContainerRef.getBoundingClientRect();
      const vertical = options.axis === 'y';
      const rows = vertical ? Array.from(tabsContainerRef.querySelectorAll<HTMLElement>('[data-tab-id], [data-ribbon-tab-id]')).filter(el => el.dataset.groupId === options.groupId()) : [];
      const first = rows[0]?.getBoundingClientRect();
      const last = rows.at(-1)?.getBoundingClientRect();
      const VERTICAL_TOLERANCE = 8;
      const inBounds = x >= rect.left && x <= rect.right &&
                       y >= (first?.top ?? rect.top) - VERTICAL_TOLERANCE && y <= (last?.bottom ?? rect.bottom) + VERTICAL_TOLERANCE;
      if (!inBounds) {
        setInsertOffset(null);
        setInsertIdx(null);
        setReorderState(null);
        // Also drop the non-reactive copy: leaving it set would apply a stale
        // reorder when the tab is released outside the bar. Safe to clear here
        // (unlike the no-active-draggable path) because this branch only runs
        // mid-drag for this bar's own tab.
        if (pendingReorderOwner === owner) pendingReorder = null;
        return;
      }
      const result = computeInsertIndex(tabsContainerRef, vertical ? y : x, draggedTabId(), options.axis, vertical ? options.groupId() : undefined);
      setInsertIdx(result?.display ?? null);
      setInsertOffset(vertical && result ? (rows[result.display]?.getBoundingClientRect().top ?? last!.bottom) - rect.top : null);
      if (result != null) {
        const nextReorder = { groupId: options.groupId(), insertIndex: result.logical };
        setReorderState(nextReorder);
        pendingReorder = nextReorder;
        pendingReorderOwner = owner;
      }
    } else {
      setInsertOffset(null);
      setInsertIdx(null);
      setReorderState(null);
      if (pendingReorderOwner === owner) pendingReorder = null;
    }
  });

  createEffect(() => {
    if (!dndCtx?.[0]?.active?.draggable) {
      setInsertOffset(null);
      setInsertIdx(null);
      setReorderState(null);
    }
  });

  return {
    insertIdx,
    insertOffset,
  };
}

interface TabStripProps {
  tabs: () => TabType[];
  activeTabId: () => string | null;
  insertIdx: () => number | null;
  onSelectTab: (tabId: string) => void;
  onTabsContainerRef?: (el: HTMLDivElement) => void;
  renderTab: (tab: () => TabType, index: () => number) => JSX.Element;
}

const TabStrip: Component<TabStripProps> = (props) => {
  const [isOverflowing, setIsOverflowing] = createSignal(false);
  const [overflowStart, setOverflowStart] = createSignal(false);
  const [overflowEnd, setOverflowEnd] = createSignal(false);
  const [showDropdown, setShowDropdown] = createSignal(false);
  let tabsContainerRef: HTMLDivElement | undefined;
  const measureEdges = () => {
    const strip = tabsContainerRef;
    if (!strip) return;
    setOverflowStart(strip.scrollLeft > 1);
    setOverflowEnd(strip.scrollWidth - strip.clientWidth - strip.scrollLeft > 1);
  };
  let revealFrame: number | undefined;
  const revealActive = () => {
    if (revealFrame !== undefined) cancelAnimationFrame(revealFrame);
    // Overflow controls change the strip's width. Measure after their layout,
    // including when a pane shrinks without changing its selected tab.
    revealFrame = requestAnimationFrame(() => {
      const strip = tabsContainerRef;
      const tab = [...(strip?.querySelectorAll<HTMLElement>('[data-tab-id], [data-ribbon-tab-id]') ?? [])]
        .find(el => el.dataset.tabId === props.activeTabId());
      if (!strip || !tab) return;
      const bounds = strip.getBoundingClientRect();
      const selected = tab.getBoundingClientRect();
      if (selected.right > bounds.right) strip.scrollLeft += selected.right - bounds.right;
      else if (selected.left < bounds.left) strip.scrollLeft -= bounds.left - selected.left;
      measureEdges();
    });
  };
  onCleanup(() => { if (revealFrame !== undefined) cancelAnimationFrame(revealFrame); });

  onMount(() => {
    if (!tabsContainerRef) return;
    const checkOverflow = () => {
      if (tabsContainerRef) {
        setIsOverflowing(tabsContainerRef.scrollWidth > tabsContainerRef.clientWidth);
        measureEdges();
        revealActive();
      }
    };
    const observer = new ResizeObserver(checkOverflow);
    observer.observe(tabsContainerRef);
    document.fonts?.addEventListener('loadingdone', checkOverflow);
    createEffect(() => {
      props.tabs();
      checkOverflow();
    });
    onCleanup(() => {
      observer.disconnect();
      document.fonts?.removeEventListener('loadingdone', checkOverflow);
    });
  });

  // Follow the active tab: newly opened or switched-to tabs scroll into
  // view instead of hiding past the strip's overflow edge.
  createEffect(() => {
    const activeId = props.activeTabId();
    if (!activeId) return;
    revealActive();
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
    <>
      <div
        ref={(el) => {
          tabsContainerRef = el;
          props.onTabsContainerRef?.(el);
        }}
        class="wm-tabstrip flex-1 flex items-end overflow-x-auto scrollbar-hide min-w-0 [scrollbar-width:none] [-ms-overflow-style:none]"
        data-overflow-start={overflowStart() ? '' : undefined}
        data-overflow-end={overflowEnd() ? '' : undefined}
        onScroll={measureEdges}
      >
        {/* Keyed by tab id, NOT object identity: updateTab replaces the tab
            object on every write (dirty flag, title), and a remounting row
            re-registers its solid-dnd draggable under the same id — the old
            row's cleanup then deletes the NEW registration, leaving the tab
            silently undraggable ("Cannot remove nonexistent draggable" at
            unmount). Key keeps the row alive across object replacement. */}
        <Key each={props.tabs()} by={(t) => t.id}>
          {(tab, i) => (
            <>
              <Show when={props.insertIdx() === i()}>
                <InsertIndicator />
              </Show>
              {props.renderTab(tab, i)}
            </>
          )}
        </Key>
        <Show when={props.insertIdx() === props.tabs().length}>
          <InsertIndicator />
        </Show>
      </div>
      <Show when={isOverflowing()}>
        <div class="relative flex-shrink-0">
          <button
            class="wm-tabbar-btn flex-shrink-0 flex items-center justify-center"
            aria-label="Show all tabs"
            onClick={(e) => { e.stopPropagation(); setShowDropdown(!showDropdown()); }}
            title="Show all tabs"
          >
            <ChevronDown class="w-3.5 h-3.5" />
          </button>
          <Show when={showDropdown()}>
            <div class="wm-tab-menu absolute right-0 top-full z-50 min-w-[160px] max-w-[280px] max-h-[300px] overflow-y-auto">
              <For each={props.tabs()}>
                {(tab) => (
                  <button
                    class="wm-tab-menu-item w-full text-left truncate"
                    data-active={tab.id === props.activeTabId() ? '' : undefined}
                    onClick={() => {
                      props.onSelectTab(tab.id);
                      setShowDropdown(false);
                      const tabEl = tabsContainerRef?.querySelector(`[data-tab-id="${tab.id}"]`);
                      tabEl?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'nearest' });
                    }}
                  >
                    {tab.title}
                    {tab.isModified && <span class="wm-tab-menu-dot">●</span>}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
    </>
  );
};

// ── Tab context menu ────────────────────────────────────────────────────

/**
 * Right-click menu on a tab: Close / Close Others / Close to the Right —
 * the classic victims of browser-owned keybinds (Ctrl+W cannot be
 * intercepted, so the menu is their discoverable home). Every close routes
 * through the dirty-tab confirm guard; a declined confirm skips that tab and
 * continues with the rest.
 *
 * The menu also moves the tab between the layout and a floating window, as
 * the panel menu of an Adobe app does. A tab in a docked pane shows "Pop out".
 * A tab in a floating window shows "Dock". Both call the store paths that the
 * pop-out button and the dock control of the window call.
 *
 * The ribbon uses this menu too, because a theme can hide the tab bars of a
 * rail. Then the ribbon icon is the only handle of the tab.
 */
export const TabContextMenu: Component<{
  groupId: () => string;
  /** The pane that shows the group. Absent for a floating group. */
  paneId?: () => string | undefined;
  tab: TabType;
  children: JSX.Element;
}> = (props) => {
  const floating = () =>
    windowStore.floatingWindows.find((w) => w.tabGroupId === props.groupId());
  const paneId = () => (floating() ? undefined : props.paneId?.() || undefined);
  const railPane = () => findEdgePanelForGroup(props.groupId()) && paneId()
    ? windowActions.findPaneById(paneId()!) : undefined;
  const canToggleFold = () => {
    const pane = railPane();
    return pane && (pane.collapsed || windowActions.canCollapsePane(pane.id));
  };
  const closesAny = (mode: 'close-others' | 'close-right') => {
    const group = windowStore.tabGroups[props.groupId()];
    return !!group && tabsToClose(group.tabs, props.tab.id, mode).some((t) => windowActions.canCloseTab(group.id, t.id));
  };
  const onSelect = (action: TabMenuAction) => {
    switch (action) {
      case 'toggle-pane-fold': {
        const pane = railPane();
        if (pane) windowActions.togglePaneCollapsed(pane.id);
        return;
      }
      case 'pop-out': {
        const pane = paneId();
        if (pane) windowActions.popOutPane(pane, props.tab.id);
        return;
      }
      case 'dock': {
        const w = floating();
        if (w) windowActions.dockFloatingWindow(w.id, props.tab.id);
        return;
      }
      default: {
        const group = windowStore.tabGroups[props.groupId()];
        if (!group) return;
        for (const t of tabsToClose(group.tabs, props.tab.id, action)) {
          if (confirmTabClose(t)) windowActions.removeTab(props.groupId(), t.id);
        }
      }
    }
  };
  return (
    <Menu.Root onSelect={(d) => onSelect(d.value as TabMenuAction)}>
      {/* asChild div: the default trigger is a BUTTON and TabItem carries its
          own close button — button-in-button is invalid HTML. */}
      <Menu.ContextTrigger
        asChild={(triggerProps) => (
          <div {...triggerProps({ class: 'contents' })}>{props.children}</div>
        )}
      />
      {/* Portaled: an in-flow positioner adds phantom layout inside the tab
          strip (it scrolled the tabs and broke pointer hit-testing). */}
      <Portal>
        <Menu.Positioner>
          <Menu.Content class={`${menuContent} z-50`}>
          {/* No Close on a tab that the policy keeps. The store refuses to
              close it, so the row would do nothing. The two bulk closes stay:
              they skip the kept tab and close the rest. */}
          <Show when={windowActions.canCloseTab(props.groupId(), props.tab.id)}>
            <Menu.Item
              value="close"
              class={menuItem}
            >
              Close
            </Menu.Item>
          </Show>
          {/* A bulk close shows only when it closes at least one tab. On a
              group of one kept tab, the rows would do nothing. */}
          <Show when={closesAny('close-others')}>
            <Menu.Item
              value="close-others"
              class={menuItem}
            >
              Close Others
            </Menu.Item>
          </Show>
          <Show when={closesAny('close-right')}>
            <Menu.Item
              value="close-right"
              class={menuItem}
            >
              Close to the Right
            </Menu.Item>
          </Show>
          <Show when={canToggleFold()}>
            <Menu.Item value="toggle-pane-fold" class={menuItem}>
              {railPane()?.collapsed ? 'Unfold pane' : 'Fold pane'}
            </Menu.Item>
          </Show>
          {/* No Pop out on a tab that the policy keeps, or that the policy
              calls unavailable. The store refuses both, so the row would do
              nothing. */}
          <Show when={paneId() && windowActions.canPopOutTab(props.groupId(), props.tab.id)}>
            <Menu.Item
              value="pop-out"
              class={menuItem}
            >
              Pop out
            </Menu.Item>
          </Show>
          <Show when={floating()}>
            <Menu.Item
              value="dock"
              class={menuItem}
            >
              Dock
            </Menu.Item>
          </Show>
          </Menu.Content>
        </Menu.Positioner>
      </Portal>
    </Menu.Root>
  );
};

// ── Center TabBar ───────────────────────────────────────────────────────

const CenterTabBar: Component<{
  groupId: string;
  paneId: string;
  onPopOut?: () => void;
}> = (props) => {
  const group = () => windowStore.tabGroups[props.groupId];
  const tabs = () => group()?.tabs ?? [];
  const activeTabId = () => group()?.activeTabId ?? null;
  // Panes render inside the center tiling AND inside edge-panel trees; the
  // bar's focus (and its e2e-visible identity) follows the group's region.
  // Memoized: the walk reads all three edge trees, and it feeds isFocused,
  // the container testid, and every tab row's testid.
  const edgePos = createMemo(() => findEdgePanelForGroup(props.groupId));
  const isFocused = () =>
    windowStore.activePaneId === props.paneId &&
    windowStore.focusedRegion === (edgePos() ?? 'center');

  let tabsContainerRef: HTMLDivElement | undefined;

  const droppable = createDroppable(`tabgroup:${props.groupId}`, {
    type: 'tabGroup',
    groupId: props.groupId,
  });

  const { insertIdx } = useTabBarDnD({
    groupId: () => props.groupId,
    tabsContainerRef: () => tabsContainerRef,
  });

  // With `merged` chrome, the bar of a floating window holds the window's
  // controls, and its empty part moves the window (FloatingWindow reads the
  // drag handle attribute).
  const floatingWindow = useFloatingWindow();
  const merged = () => floatingWindow?.chrome() === 'merged';

  return (
    <div
      use:droppable
      ref={attachNativeMenuGuard}
      {...(edgePos() ? { 'data-testid': `edge-tabbar-${edgePos()}` } : {})}
      data-drop-active={droppable.isActiveDroppable ? '' : undefined}
      data-wm-drag-handle={merged() ? '' : undefined}
      // Re-asserts the shell's select-none: the strip is drag chrome, and a
      // pane body re-enabled selection beneath this bar. `h-9` is a layout
      // contract: COLLAPSED_PANE_PX and the ribbon's MARKER_PX are 36px.
      class="wm-tabbar flex-shrink-0 flex items-center h-9 relative select-none"
    >
      <TabStrip
        tabs={tabs}
        activeTabId={activeTabId}
        insertIdx={insertIdx}
        onSelectTab={(tabId) => windowActions.setActiveTab(props.groupId, tabId)}
        onTabsContainerRef={(el) => {
          tabsContainerRef = el;
        }}
        renderTab={(tab) => (
          <TabContextMenu groupId={() => props.groupId} paneId={() => props.paneId} tab={tab()}>
            <TabItem
              tab={tab()}
              draggableId={`tab:${props.groupId}:${tab().id}`}
              draggableData={{ type: 'tab', tab: tab(), sourceGroupId: props.groupId }}
              isActive={tab().id === activeTabId()}
              isFocused={isFocused()}
              onClick={() => windowActions.setActiveTab(props.groupId, tab().id)}
              onClose={() => confirmTabClose(tab()) && windowActions.removeTab(props.groupId, tab().id)}
              closable={windowActions.canCloseTab(props.groupId, tab().id)}
              testId={edgePos() ? `edge-tab-${edgePos()}-${tab().id}` : undefined}
            />
          </TabContextMenu>
        )}
      />
      <div class="wm-tabbar-actions flex-shrink-0 flex items-center">
        {/* Edge bars stay minimal (Obsidian sidebars have no pop-out) —
            edge content reaches floating windows by dragging the tab, or by
            the Pop out row of the tab menu. */}
        {!edgePos() && props.onPopOut && tabs().length > 0 && (
          <button
            onClick={props.onPopOut}
            class="wm-tabbar-btn flex items-center justify-center"
            title="Pop out to floating window"
            aria-label="Pop out to floating window"
          >
            <IconPopOut class="w-4 h-4" />
          </button>
        )}
        <Show when={floatingWindow && merged()}>
          <WindowControls windowId={floatingWindow!.id} />
        </Show>
      </div>
      {droppable.isActiveDroppable && (
        <div class="wm-tabbar-drop-line absolute inset-x-0 bottom-0" />
      )}
    </div>
  );
};

// ── Exported TabBar ─────────────────────────────────────────────────────
// One bar for every region: edge panels host the same Pane/TabBar stack as
// the center tiling (their tab bars self-identify via the group's region).

export const TabBar: Component<TabBarProps> = (props) => {
  // Keyed: CenterTabBar registers its `tabgroup:` droppable with the group
  // id captured at mount. Layout restores and group churn swap the id under
  // a surviving instance, leaving a stale drop target — remount instead.
  return (
    <Show when={props.groupId} keyed>
      {(groupId) => (
        <CenterTabBar
          groupId={groupId}
          paneId={props.paneId}
          onPopOut={props.onPopOut}
        />
      )}
    </Show>
  );
};
