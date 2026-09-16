import { Component, Show, createMemo, onCleanup, untrack } from 'solid-js';
import { createDroppable, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from './TabBar';
import { EmptyPane } from './EmptyPane';
import { windowStore, windowActions } from '@/windowing/store';
import { regionOfPane } from '@/windowing/model/tree';
import { hasTabsOutsidePane } from '@/windowing/model/pane-content';
import { useWindowing } from '@/windowing/components/context';

type PaneDropPosition = 'left' | 'right' | 'top' | 'bottom';

function PaneDropZone(props: {
  position: PaneDropPosition;
  droppable: ReturnType<typeof createDroppable>;
  class: string;
}) {
  const droppable = props.droppable;
  return (
    <div
      use:droppable
      classList={{
        [props.class]: true,
        'bg-primary/30': droppable.isActiveDroppable,
      }}
    />
  );
}

export const Pane: Component<{ paneId: string }> = (props) => {
  const windowing = useWindowing();
  const dndContext = useDragDropContext();
  // Match by payload type, not draggable-id prefix: anything carrying a Tab
  // ('tab' moves, 'newTab' spawns from e.g. a hover card) targets panes.
  const isTabDragging = () => {
    if (!dndContext) return false;
    const [dndState] = dndContext;
    const type = (dndState.active.draggable?.data as { type?: string } | undefined)?.type;
    return type === 'tab' || type === 'newTab';
  };

  const tabGroupId = () => windowActions.findPaneById(props.paneId)?.tabGroupId ?? null;
  const group = () => (tabGroupId() ? windowStore.tabGroups[tabGroupId()!] : null);
  const tabs = () => group()?.tabs ?? [];
  const activeTab = () => {
    const g = group();
    if (!g?.activeTabId) return null;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? null;
  };
  const centerDroppable = createDroppable(`pane:${props.paneId}:center`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'center',
  });

  const leftDroppable = createDroppable(`pane:${props.paneId}:left`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'left',
  });
  const rightDroppable = createDroppable(`pane:${props.paneId}:right`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'right',
  });
  const topDroppable = createDroppable(`pane:${props.paneId}:top`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'top',
  });
  const bottomDroppable = createDroppable(`pane:${props.paneId}:bottom`, {
    type: 'pane',
    paneId: props.paneId,
    position: 'bottom',
  });

  // Native drags from outside the window manager (the app's file tree) reach
  // the pane body through the app's drop target. The app marks the body with
  // `data-file-drop-over` while a drag hovers it.
  const attachDrop = (el: HTMLElement) => {
    const cleanup = windowing.slots.attachDropTarget?.(el, tabGroupId);
    onCleanup(() => cleanup?.());
  };

  // Pop-out MOVES the group (popOutPane detaches it from this pane) — sharing
  // one group between a pane and a floating window would register duplicate
  // solid-dnd draggable ids and mirror the tab strip in two places.
  const handlePopOut = () => {
    windowActions.popOutPane(props.paneId);
  };

  // Re-render the panel only when the active tab's identity or content type
  // changes — NOT when unrelated tab fields (e.g. isModified) churn the tab
  // object reference. updateTab() replaces the whole tabs array on every write,
  // so depending on activeTab() directly would remount the panel (and, for the
  // editor, discard in-progress edits + loop). Metadata is NOT write-once, so
  // it reaches the panel through `reactiveMetadataProps` instead: per-key
  // memos, which deliver a later write without re-running this.
  const activeTabId = createMemo(() => activeTab()?.id ?? null);
  const activeContentType = createMemo(() => activeTab()?.contentType ?? null);

  const renderContent = () => {
    const id = activeTabId();
    const contentType = activeContentType();
    if (!id || !contentType) {
      return (
        <div class="flex-1 flex items-center justify-center bg-shell-bg">
          <div class="text-muted-dark text-sm">No tab selected</div>
        </div>
      );
    }
    // The renderer gets the LIVE tab, and runs untracked: a read inside it must
    // not make this function re-run, which would remount the panel.
    const snapshot = untrack(activeTab)!;
    return untrack(() => windowing.renderContent(() => activeTab() ?? snapshot));
  };

  // A collapsed rail pane is CLIPPED to its tab strip, not unmounted: the
  // parent split gives it the strip's height and `overflow-hidden` takes the
  // rest. Unmounting would tear down the shell (and its scrollback) every time
  // the user tucked the terminal away.
  const collapsed = () => windowActions.findPaneById(props.paneId)?.collapsed === true;

  // The affordance belongs to the centre tiling only. A rail pane is one slot
  // of a fixed tool stack; "open a note here" is not an instruction it can
  // honour, and the ribbon already marks it.
  const inCenter = createMemo(() => regionOfPane(windowStore, props.paneId) === 'center');
  const solitary = () =>
    !hasTabsOutsidePane(windowStore.tabGroups, windowStore.layout, props.paneId);

  const handleClick = () => {
    windowActions.setActivePane(props.paneId);
    // The bar IS the affordance: clicking anywhere on a collapsed pane opens
    // it, so the ribbon marker is a shortcut rather than the only way back.
    // No region lookup — a pane id is unique across every root, and a collapsed
    // pane in the centre tiling has to reopen the same way one in a rail does.
    if (collapsed()) windowActions.setPaneCollapsed(props.paneId, false);
  };

  return (
    <div
      use:centerDroppable
      ref={attachDrop}
      data-pane-id={props.paneId}
      data-pane-collapsed={collapsed() ? 'true' : undefined}
      classList={{
        'relative flex flex-col h-full overflow-hidden transition-all data-file-drop-over:ring-1 data-file-drop-over:ring-primary/60': true,
        // Focus reads through the active tab chip (Obsidian's language) —
        // no colored ring around the pane itself.
        'bg-primary/5': centerDroppable.isActiveDroppable,
        'cursor-pointer': collapsed(),
      }}
      onClick={handleClick}
    >
      {/* A pane with no tabs holds no splash. It holds one quiet affordance
          that names the state and the rows the app gives it, because a region
          that draws nothing at all reads as a rendering failure. It is still a
          drop target. */}
      <Show when={tabs().length > 0} fallback={<Show when={inCenter()}><EmptyPane solitary={solitary()} /></Show>}>
        <TabBar
          groupId={tabGroupId()!}
          paneId={props.paneId}
          onPopOut={handlePopOut}
        />
        {renderContent()}
      </Show>

      <Show when={centerDroppable.isActiveDroppable}>
        <div class="absolute inset-0 bg-primary/20 z-10 pointer-events-none cru-anim-fade" />
      </Show>

      <div
        classList={{
          'absolute inset-0 z-20': true,
          'pointer-events-auto': isTabDragging(),
          'pointer-events-none': !isTabDragging(),
        }}
      >
        <PaneDropZone
          position="top"
          droppable={topDroppable}
          class="absolute top-0 left-0 right-0 h-1/5 min-h-[24px]"
        />
        <PaneDropZone
          position="bottom"
          droppable={bottomDroppable}
          class="absolute bottom-0 left-0 right-0 h-1/5 min-h-[24px]"
        />
        <PaneDropZone
          position="left"
          droppable={leftDroppable}
          class="absolute top-0 bottom-0 left-0 w-1/5 min-w-[24px]"
        />
        <PaneDropZone
          position="right"
          droppable={rightDroppable}
          class="absolute top-0 bottom-0 right-0 w-1/5 min-w-[24px]"
        />
      </div>
    </div>
  );
};
