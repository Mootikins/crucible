import { Component, createSignal, createEffect, onCleanup, For } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import type { FloatingWindow as FloatingWindowType } from '@/types/windowTypes';
import { TabBar } from './TabBar';
import { IconClose, IconMinimize } from './icons';

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
  const [isDragging, setIsDragging] = createSignal(false);
  const [dragStart, setDragStart] = createSignal({ x: 0, y: 0, windowX: 0, windowY: 0 });
  const [isHovered, setIsHovered] = createSignal(false);

  const handleResizePointerDown = (edge: ResizeEdge, e: PointerEvent) => {
    if (w().isMaximized) return;
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
    };

    document.addEventListener('pointermove', onMove);
    document.addEventListener('pointerup', onUp);
  };

  const handleTitleMouseDown = (e: MouseEvent) => {
    if (w().isMaximized) return;
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

  return (
    <div
      class="absolute flex flex-col bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl"
      style={{
        left: `${w().x}px`,
        top: `${w().y}px`,
        width: `${w().width}px`,
        height: `${w().height}px`,
        'z-index': `${w().zIndex}`,
      }}
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
      <div
        class="flex items-center justify-between px-2 py-1 bg-zinc-800 border-b border-zinc-700 cursor-grab active:cursor-grabbing select-none"
        onMouseDown={handleTitleMouseDown}
      >
         <span class="text-xs font-medium text-zinc-300 truncate">
           {w().title ?? 'Window'}
         </span>
        <div class="flex items-center gap-0.5">
          <button
            type="button"
            class="p-1 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-700"
            onClick={() => windowActions.minimizeFloatingWindow(w().id)}
            title="Minimize"
          >
            <IconMinimize class="w-3 h-3" />
          </button>
          <button
            type="button"
            class="p-1 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-700"
            onClick={() => windowActions.removeFloatingWindow(w().id)}
            title="Close"
          >
            <IconClose class="w-3 h-3" />
          </button>
        </div>
      </div>
      <div class="flex-1 flex flex-col min-h-0 overflow-hidden">
        <TabBar groupId={w().tabGroupId} paneId="" />
        <div class="flex-1 bg-zinc-900 overflow-auto p-2 text-xs text-zinc-400">
          {tabs().length > 0 ? (
            <span>Content for {tabs()[0]?.title}</span>
          ) : (
            <span>No tabs</span>
          )}
        </div>
      </div>
    </div>
  );
};
