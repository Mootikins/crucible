import { Component, createSignal } from 'solid-js';
import { Pane } from './Pane';
import { windowStore, setStore, updateSplitRatio } from '@/stores/windowStore';
import type { LayoutNode } from '@/types/windowTypes';
import { IconGripVertical, IconGripHorizontal } from './icons';

export const SplitPane: Component<{ node: LayoutNode }> = (props) => {
  const node = () => props.node;
  if (node().type === 'pane') {
    return <Pane paneId={node().id} />;
  }

  const split = () => node() as Extract<LayoutNode, { type: 'split' }>;
  const [localRatio, setLocalRatio] = createSignal(split().splitRatio);
  const [isDragging, setIsDragging] = createSignal(false);
  let containerRef: HTMLDivElement;

  const effectiveRatio = () => (isDragging() ? localRatio() : split().splitRatio);

  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    const startX = e.clientX;
    const startY = e.clientY;
    const startRatio = localRatio();
    const containerRect = () => containerRef?.getBoundingClientRect();

    const handleMouseMove = (e: MouseEvent) => {
      const rect = containerRect();
      if (!rect) return;
      let newRatio: number;
      if (split().direction === 'horizontal') {
        newRatio = startRatio + (e.clientX - startX) / rect.width;
      } else {
        newRatio = startRatio + (e.clientY - startY) / rect.height;
      }
      setLocalRatio(Math.max(0.1, Math.min(0.9, newRatio)));
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      const newLayout = updateSplitRatio(
        windowStore.layout,
        split().id,
        localRatio()
      );
      setStore('layout', newLayout);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  const s = split();
  const dir = s.direction;
  const isHoriz = dir === 'horizontal';
  const ratio = effectiveRatio();

  return (
    <div
      ref={(el) => (containerRef = el)}
      classList={{
        'flex h-full w-full': true,
        'flex-row': isHoriz,
        'flex-col': !isHoriz,
      }}
    >
      <div
        class="overflow-hidden"
        style={{
          [isHoriz ? 'width' : 'height']: `${ratio * 100}%`,
          [isHoriz ? 'height' : 'width']: '100%',
        }}
      >
        {s.first.type === 'pane' ? (
          <Pane paneId={s.first.id} />
        ) : (
          <SplitPane node={s.first} />
        )}
      </div>
      <div
        classList={{
          'flex-shrink-0 group relative z-10 transition-colors': true,
          'w-1 cursor-col-resize hover:w-1.5 active:w-1.5': isHoriz,
          'h-1 cursor-row-resize hover:h-1.5 active:h-1.5': !isHoriz,
          'bg-blue-500': isDragging(),
          'bg-zinc-800 hover:bg-zinc-700 active:bg-blue-500': !isDragging(),
        }}
        onMouseDown={handleMouseDown}
      >
        <div
          classList={{
            'absolute opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-zinc-500 group-hover:text-zinc-400': true,
            'inset-y-0 left-1/2 -translate-x-1/2': isHoriz,
            'inset-x-0 top-1/2 -translate-y-1/2': !isHoriz,
          }}
        >
          {isHoriz ? (
            <IconGripVertical class="w-3 h-6" />
          ) : (
            <IconGripHorizontal class="w-6 h-3" />
          )}
        </div>
      </div>
      <div
        class="overflow-hidden"
        style={{
          [isHoriz ? 'width' : 'height']: `${(1 - ratio) * 100}%`,
          [isHoriz ? 'height' : 'width']: '100%',
        }}
      >
        {s.second.type === 'pane' ? (
          <Pane paneId={s.second.id} />
        ) : (
          <SplitPane node={s.second} />
        )}
      </div>
    </div>
  );
};
