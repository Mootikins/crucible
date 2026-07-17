import { Component, Show, createMemo, createSignal, createEffect, onCleanup, untrack, For } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { FloatingWindow as FloatingWindowType } from '@/types/windowTypes';
import { TabBar } from './TabBar';
import {
  IconClose,
  IconLayout,
  IconMinimize,
  IconMaximize,
  IconPin,
  IconTabBar,
} from './icons';
import { confirmTabClose } from '@/lib/tab-guards';
import { getGlobalRegistry } from '@/lib/panel-registry';

type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'nw' | 'ne' | 'sw' | 'se';

const MIN_WIDTH = 200;
const MIN_HEIGHT = 150;

const HANDLE_DEFS: { edge: ResizeEdge; cursor: string; style: Record<string, string> }[] = [
  // Edges (6px strips)
  { edge: 'n',  cursor: 'n-resize',  style: { top: '-2px', left: '12px', right: '12px', height: '6px', 'z-index': '1' } },
  { edge: 's',  cursor: 's-resize',  style: { bottom: '-2px', left: '12px', right: '12px', height: '6px', 'z-index': '1' } },
  { edge: 'w',  cursor: 'w-resize',  style: { left: '-2px', top: '12px', bottom: '12px', width: '6px', 'z-index': '1' } },
  { edge: 'e',  cursor: 'e-resize',  style: { right: '-2px', top: '12px', bottom: '12px', width: '6px', 'z-index': '1' } },
  // Corners (12x12, higher z-index)
  { edge: 'nw', cursor: 'nw-resize', style: { top: '-2px', left: '-2px', width: '12px', height: '12px', 'z-index': '2' } },
  { edge: 'ne', cursor: 'ne-resize', style: { top: '-2px', right: '-2px', width: '12px', height: '12px', 'z-index': '2' } },
  { edge: 'sw', cursor: 'sw-resize', style: { bottom: '-2px', left: '-2px', width: '12px', height: '12px', 'z-index': '2' } },
  { edge: 'se', cursor: 'se-resize', style: { bottom: '-2px', right: '-2px', width: '12px', height: '12px', 'z-index': '2' } },
];

export const FloatingWindow: Component<{ window: FloatingWindowType }> = (props) => {
  const w = () => props.window;
  const group = () => windowStore.tabGroups[w().tabGroupId];
  const tabs = () => group()?.tabs ?? [];
  const activeTab = () => {
    const g = group();
    if (!g) return null;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? g.tabs[0] ?? null;
  };
  // Same guard as Pane.renderContent: re-render the panel only when the
  // active tab's identity/type changes, NOT when updateTab churns the tab
  // object (dirty-flag sync would otherwise remount the editor in a loop).
  const activeTabId = createMemo(() => activeTab()?.id ?? null);
  const activeContentType = createMemo(() => activeTab()?.contentType ?? null);
  const [isDragging, setIsDragging] = createSignal(false);
  const [dragStart, setDragStart] = createSignal({ x: 0, y: 0, windowX: 0, windowY: 0 });
  const [isHovered, setIsHovered] = createSignal(false);
  let resizeCleanup: (() => void) | null = null;
  onCleanup(() => {
    resizeCleanup?.();
    resizeCleanup = null;
  });

  // Hover Editor auto-pin: interacting with a transient popover's geometry
  // (drag or resize) means the user wants to keep it.
  const autoPin = () => {
    if (w().transient) windowActions.pinFloatingWindow(w().id);
  };

  const handleResizePointerDown = (edge: ResizeEdge, e: PointerEvent) => {
    if (w().isMaximized) return;
    autoPin();
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);

    const startX = e.clientX;
    const startY = e.clientY;
    const startWinX = w().x;
    const startWinY = w().y;
    const startW = w().width;
    const startH = w().height;
    const winId = w().id;

    const affectsLeft = edge.includes('w');
    const affectsTop = edge.includes('n');
    const movesWidth = edge === 'e' || edge === 'w' || edge.length === 2;
    const movesHeight = edge === 'n' || edge === 's' || edge.length === 2;

    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      const dy = ev.clientY - startY;

      let newX = startWinX;
      let newY = startWinY;
      let newW = startW;
      let newH = startH;

      if (movesWidth) {
        if (affectsLeft) {
          newW = Math.max(MIN_WIDTH, startW - dx);
          newX = startWinX + startW - newW;
          if (newX < 0) { newW += newX; newX = 0; newW = Math.max(MIN_WIDTH, newW); }
        } else {
          newW = Math.max(MIN_WIDTH, startW + dx);
        }
      }

      if (movesHeight) {
        if (affectsTop) {
          newH = Math.max(MIN_HEIGHT, startH - dy);
          newY = startWinY + startH - newH;
          if (newY < 0) { newH += newY; newY = 0; newH = Math.max(MIN_HEIGHT, newH); }
        } else {
          newH = Math.max(MIN_HEIGHT, startH + dy);
        }
      }

      windowActions.updateFloatingWindow(winId, { x: newX, y: newY, width: newW, height: newH });
    };

    const onUp = (ev: PointerEvent) => {
      el.releasePointerCapture(ev.pointerId);
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
      resizeCleanup = null;
    };

    document.addEventListener('pointermove', onMove);
    document.addEventListener('pointerup', onUp);
    resizeCleanup = () => {
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
    };
  };

  // Closing the window closes its tabs — same unsaved-changes contract as
  // every other tab-close path (confirmTabClose per modified tab).
  const handleClose = () => {
    const modified = tabs().filter((t) => t.isModified);
    for (const tab of modified) {
      if (!confirmTabClose(tab)) return;
    }
    windowActions.closeFloatingWindow(w().id);
  };

  const handleTitleMouseDown = (e: MouseEvent) => {
    if (w().isMaximized) return;
    autoPin();
    e.preventDefault();
    setIsDragging(true);
    setDragStart({
      x: e.clientX,
      y: e.clientY,
      windowX: w().x,
      windowY: w().y,
    });
    windowActions.bringToFront(w().id);
  };

  createEffect(() => {
    if (!isDragging()) return;
    const move = (e: MouseEvent) => {
      const start = dragStart();
      windowActions.updateFloatingWindow(w().id, {
        x: Math.max(0, start.windowX + (e.clientX - start.x)),
        y: Math.max(0, start.windowY + (e.clientY - start.y)),
      });
    };
    const up = () => setIsDragging(false);
    document.addEventListener('mousemove', move);
    document.addEventListener('mouseup', up);
    onCleanup(() => {
      document.removeEventListener('mousemove', move);
      document.removeEventListener('mouseup', up);
    });
  });

  const titleBtn =
    'p-0.5 rounded text-muted-dark hover:text-shell-body hover:bg-hover-wash';

  return (
    <div
      data-window-id={w().id}
      class="absolute flex flex-col bg-surface-overlay border border-hairline-strong rounded-md shadow-lg"
      style={
        w().isMaximized
          ? {
              // Hover Editor maximize: cover the workspace, keeping the
              // far-left ribbon, header, and status bar visible.
              left: '44px',
              top: '44px',
              width: 'calc(100vw - 48px)',
              height: 'calc(100vh - 72px)',
              'z-index': `${w().zIndex}`,
            }
          : {
              left: `${w().x}px`,
              top: `${w().y}px`,
              width: `${w().width}px`,
              height: `${w().height}px`,
              'z-index': `${w().zIndex}`,
            }
      }
      onMouseDown={() => windowActions.bringToFront(w().id)}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      <For each={HANDLE_DEFS}>
        {(h) => (
          <div
            style={{
              position: 'absolute',
              ...h.style,
              cursor: h.cursor,
              opacity: isHovered() && !w().isMaximized ? '1' : '0',
            }}
            onPointerDown={(e) => handleResizePointerDown(h.edge, e)}
          />
        )}
      </For>
      {/* Hover Editor titlebar: pin | title (drag) | tab-bar toggle, dock,
          roll up, maximize/restore, close. */}
      <div
        class="flex h-7 items-center gap-1 border-b border-hairline bg-surface-overlay px-1.5 cursor-grab active:cursor-grabbing select-none"
        onMouseDown={handleTitleMouseDown}
      >
        <Show when={w().transient}>
          <button
            type="button"
            class={titleBtn}
            data-testid="float-pin"
            onClick={(e) => {
              e.stopPropagation();
              windowActions.pinFloatingWindow(w().id);
            }}
            onMouseDown={(e) => e.stopPropagation()}
            title="Pin (keep open when the cursor leaves)"
          >
            <IconPin class="w-3 h-3" />
          </button>
        </Show>
        <span class="min-w-0 flex-1 truncate text-xs font-medium text-shell-body">
          {w().title ?? 'Window'}
        </span>
        <div class="flex items-center gap-0.5" onMouseDown={(e) => e.stopPropagation()}>
          <button
            type="button"
            class={titleBtn}
            data-testid="float-tabbar-toggle"
            onClick={() =>
              windowActions.updateFloatingWindow(w().id, {
                showTabBar: w().showTabBar === false,
              })
            }
            title={w().showTabBar === false ? 'Show tab bar' : 'Hide tab bar'}
          >
            <IconTabBar class="w-3 h-3" />
          </button>
          <button
            type="button"
            class={titleBtn}
            onClick={() => windowActions.dockFloatingWindow(w().id)}
            title="Dock back into the layout"
          >
            <IconLayout class="w-3 h-3" />
          </button>
          <button
            type="button"
            class={titleBtn}
            onClick={() => windowActions.minimizeFloatingWindow(w().id)}
            title="Roll up into the window bar"
          >
            <IconMinimize class="w-3 h-3" />
          </button>
          <button
            type="button"
            class={titleBtn}
            data-testid="float-maximize"
            onClick={() =>
              w().isMaximized
                ? windowActions.restoreFloatingWindow(w().id)
                : windowActions.maximizeFloatingWindow(w().id)
            }
            title={w().isMaximized ? 'Restore previous size' : 'Maximize'}
          >
            <IconMaximize class="w-3 h-3" />
          </button>
          <button type="button" class={titleBtn} onClick={handleClose} title="Close (closes its tabs)">
            <IconClose class="w-3 h-3" />
          </button>
        </div>
      </div>
      <div class="flex-1 flex flex-col min-h-0 overflow-hidden">
        <Show when={w().showTabBar !== false}>
          <TabBar mode="center" groupId={w().tabGroupId} paneId="" />
        </Show>
        <div class="flex-1 bg-surface-base overflow-auto p-2 text-xs text-muted" data-testid={`panel-content-${activeContentType() ?? 'unknown'}`}>
          {(() => {
            const id = activeTabId();
            const contentType = activeContentType();
            if (!id || !contentType) {
              return (
                <div class="flex-1 bg-surface-base overflow-auto p-2 text-xs text-muted">
                  <span>No tabs</span>
                </div>
              );
            }
            const tab = untrack(() => activeTab());
            const panel = getGlobalRegistry().get(contentType);
            if (panel) {
              const panelProps = (tab?.metadata ?? {}) as Record<string, unknown>;
              return <Dynamic component={panel.component} {...panelProps} />;
            }
            return (
              <div class="flex-1 bg-surface-base overflow-auto p-2 text-xs text-muted">
                <span>Content for {tab?.title}</span>
              </div>
            );
          })()}
        </div>
      </div>
    </div>
  );
};
