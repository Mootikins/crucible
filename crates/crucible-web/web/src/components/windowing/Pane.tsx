import { Component, Show, createMemo, createSignal, onCleanup, untrack } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { createDroppable, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from './TabBar';
import { windowStore, windowActions } from '@/stores/windowStore';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { attachFileDropTarget } from '@/lib/file-dnd';
import { openFileInGroup } from '@/lib/file-actions';
import { EmptyState } from '@/components/EmptyState';

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
  const isActive = () => windowStore.activePaneId === props.paneId;

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

  // Native file drag from the file tree (pragmatic-drag-and-drop, a separate
  // pipeline from solid-dnd tab drags): dropping a FILE on the pane opens it
  // here. The editor content area registers its own inner 'editor' zone
  // (insert-link); the innermost-zone protocol in file-dnd keeps the two
  // behaviors exclusive.
  const [fileDropOver, setFileDropOver] = createSignal(false);
  const attachFileDrop = (el: HTMLElement) => {
    const cleanup = attachFileDropTarget(el, {
      zone: 'pane',
      canDrop: (source) => !source.isDir,
      onDragEnter: () => setFileDropOver(true),
      onDragLeave: () => setFileDropOver(false),
      onDrop: (source) => {
        setFileDropOver(false);
        windowActions.setActivePane(props.paneId);
        openFileInGroup(tabGroupId(), source.absPath, source.name);
      },
    });
    onCleanup(cleanup);
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
  // editor, discard in-progress edits + loop). Metadata is read untracked since
  // it is set once at tab creation.
  const activeTabId = createMemo(() => activeTab()?.id ?? null);
  const activeContentType = createMemo(() => activeTab()?.contentType ?? null);

  const renderContent = () => {
    const id = activeTabId();
    const contentType = activeContentType();
    if (!id || !contentType) {
      return (
        <div class="flex-1 flex items-center justify-center bg-surface-base">
          <div class="text-muted-dark text-sm">No tab selected</div>
        </div>
      );
    }
    const tab = untrack(() => activeTab());
    const panel = getGlobalRegistry().get(contentType);
    if (panel) {
      const panelProps = (tab?.metadata ?? {}) as Record<string, unknown>;
      return <Dynamic component={panel.component} {...panelProps} />;
    }
    // Every shipped content type is registry-backed; anything else is a
    // stale persisted layout entry.
    return (
      <div class="flex-1 bg-shell-panel flex items-center justify-center">
        <div class="text-muted-dark text-sm">Unknown content type</div>
      </div>
    );
  };

  return (
    <div
      use:centerDroppable
      ref={attachFileDrop}
      classList={{
        'relative flex flex-col h-full overflow-hidden transition-all': true,
        'ring-1 ring-primary/30': isActive() && windowStore.focusedRegion === 'center',
        'bg-primary/5': centerDroppable.isActiveDroppable,
        'ring-1 ring-primary/60': fileDropOver(),
      }}
      onClick={() => windowActions.setActivePane(props.paneId)}
    >
      <Show
        when={tabs().length > 0}
        fallback={
          <EmptyState
            onAction={() => window.dispatchEvent(new CustomEvent('crucible:new-session'))}
          />
        }
      >
        <TabBar
          mode="center"
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
