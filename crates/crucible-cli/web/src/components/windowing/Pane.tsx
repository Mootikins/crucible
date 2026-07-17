import { Component, Show, createMemo, untrack } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { createDroppable, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from './TabBar';
import { windowStore, windowActions } from '@/stores/windowStore';
import { getGlobalRegistry } from '@/lib/panel-registry';
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
    switch (contentType) {
      case 'file':
        return (
          <div class="flex-1 bg-shell-panel overflow-auto p-4">
            <pre class="text-sm font-mono text-shell-body whitespace-pre-wrap">
              {`// ${tab?.title ?? ''}\nimport { createRoot } from 'solid-js/web'\nimport App from './App'\n\nconst root = document.getElementById('root')\nif (root) createRoot(root).render(() => <App />)\n`}
            </pre>
          </div>
        );
      case 'document':
        return (
          <div class="flex-1 bg-shell-panel overflow-auto p-6">
            <h1 class="text-2xl font-bold text-shell-ink mb-4">Document Preview</h1>
            <div class="prose prose-invert max-w-none">
              <p class="text-shell-body leading-relaxed">
                This is a sample document. Drag tabs between panes, split panes, and pop out to
                floating windows.
              </p>
            </div>
          </div>
        );
      case 'preview':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center p-8">
            <div class="bg-gradient-to-br from-surface-elevated to-surface-base rounded-xl p-8 border border-hairline">
              <div class="w-64 h-40 bg-surface-elevated rounded-lg flex items-center justify-center">
                <span class="text-muted-dark">Preview</span>
              </div>
            </div>
          </div>
        );
      case 'terminal':
        return (
          <div class="flex-1 bg-black font-mono text-sm p-3 overflow-auto">
            <div class="text-ok mb-2">$ bun run dev</div>
            <div class="text-shell-body mb-1">Starting dev server...</div>
            <div class="text-muted-dark mb-2">
              <span class="text-ok">$</span> <span class="animate-pulse">|</span>
            </div>
          </div>
        );
      case 'tool':
        return (
          <div class="flex-1 bg-shell-panel overflow-auto p-4">
          </div>
        );
      case 'explorer':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">📁 Explorer (Coming Soon)</div>
          </div>
        );
      case 'search':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">🔍 Search (Coming Soon)</div>
          </div>
        );
      case 'source-control':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">⎇ Source Control (Coming Soon)</div>
          </div>
        );
      case 'outline':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">📋 Outline (Coming Soon)</div>
          </div>
        );
      case 'problems':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">⚠️ Problems (Coming Soon)</div>
          </div>
        );
      case 'output':
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">📄 Output (Coming Soon)</div>
          </div>
        );
      default:
        return (
          <div class="flex-1 bg-shell-panel flex items-center justify-center">
            <div class="text-muted-dark text-sm">Unknown content type</div>
          </div>
        );
    }
  };

  return (
    <div
      use:centerDroppable
      classList={{
        'relative flex flex-col h-full overflow-hidden transition-all': true,
        'ring-1 ring-primary/30': isActive() && windowStore.focusedRegion === 'center',
        'bg-primary/5': centerDroppable.isActiveDroppable,
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
        <div class="absolute inset-0 bg-primary/20 z-10 pointer-events-none" />
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
