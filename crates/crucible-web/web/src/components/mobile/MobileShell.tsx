import { Component, createSignal, onCleanup, onMount } from 'solid-js';
import { ContentSurface } from '@/components/mobile/ContentSurface';
import { Drawer } from '@/components/mobile/Drawer';
import { createEdgeSwipe, type SwipePoint } from '@/components/mobile/edge-swipe';
import type { DrawerSide } from '@/components/mobile/drawer-gesture';
import { SessionsPanel } from '@/components/SessionsPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { ClipboardList, FolderTree } from '@/lib/icons';
import type { Tab } from '@/types/windowTypes';

/** `min(85vw, 320px)`, in px, because the swipe measures against it. */
const drawerWidthFor = (viewport: number) => Math.min(Math.round(viewport * 0.85), 320);

/**
 * The compact shell: an app bar over one content surface, with an edge drawer
 * on each side — sessions on the left and files on the right, as the desktop
 * rails place them. See `docs/Meta/Architecture/Mobile Shell.md`.
 *
 * The tab stack and the drawers' own tabs arrive in later steps of Track A.
 */
export const MobileShell: Component = () => {
  // Replaced by the tab stack in step 3; until then nothing opens a tab.
  const [activeTab] = createSignal<Tab | null>(null);
  const [openSide, setOpenSide] = createSignal<DrawerSide | null>(null);
  const [viewport, setViewport] = createSignal(window.innerWidth);
  const width = () => drawerWidthFor(viewport());

  onMount(() => {
    const onResize = () => setViewport(window.innerWidth);
    window.addEventListener('resize', onResize);
    onCleanup(() => window.removeEventListener('resize', onResize));
  });

  // One drawer at a time: opening a side replaces whatever was open.
  const setSide = (side: DrawerSide) => (open: boolean) =>
    setOpenSide(open ? side : openSide() === side ? null : openSide());

  const swipes = (['left', 'right'] as const).map((side) =>
    createEdgeSwipe({
      side,
      width,
      viewportWidth: viewport,
      isOpen: () => openSide() === side,
      onSettle: setSide(side),
    }),
  );
  const [leftSwipe, rightSwipe] = swipes;

  const sample = (e: PointerEvent): SwipePoint => ({
    x: e.clientX,
    y: e.clientY,
    t: e.timeStamp,
    inDrawer: e.target instanceof Element && e.target.closest('[data-drawer-part]') !== null,
  });
  const each = (fn: (s: (typeof swipes)[number], p: SwipePoint) => void) => (e: PointerEvent) => {
    const p = sample(e);
    for (const s of swipes) fn(s, p);
  };

  // The same marks the desktop gives these panels (lib/tab-icons.ts).
  const DrawerButton = (props: { side: DrawerSide; label: string; icon: typeof FolderTree }) => {
    const Icon = props.icon;
    return (
      <button
        type="button"
        aria-label={props.label}
        aria-expanded={openSide() === props.side}
        class="w-11 h-11 flex items-center justify-center shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash focus-ring"
        onClick={() => setSide(props.side)(openSide() !== props.side)}
      >
        <Icon class="w-5 h-5" />
      </button>
    );
  };

  return (
    <div
      class="flex flex-col h-dvh bg-shell-bg text-shell-ink overflow-hidden"
      data-testid="mobile-shell"
      onPointerDown={each((s, p) => s.down(p))}
      onPointerMove={each((s, p) => s.move(p))}
      onPointerUp={each((s, p) => s.up(p))}
      onPointerCancel={() => swipes.forEach((s) => s.cancel())}
    >
      <header
        class="shrink-0 flex items-center gap-1 h-12 px-1 border-b border-hairline bg-surface-base"
        style={{ 'padding-top': 'var(--inset-top)', 'box-sizing': 'content-box' }}
      >
        <DrawerButton side="left" label="Sessions" icon={ClipboardList} />
        <h1 class="flex-1 truncate text-sm font-medium text-shell-ink px-1">
          {activeTab()?.title ?? 'Crucible'}
        </h1>
        <DrawerButton side="right" label="Files" icon={FolderTree} />
      </header>
      <main
        class="flex-1 min-h-0 flex flex-col"
        // The browser keeps vertical scroll; horizontal travel reaches the swipe.
        style={{ 'padding-bottom': 'var(--inset-bottom)', 'touch-action': 'pan-y' }}
      >
        <ContentSurface
          tab={activeTab}
          empty={
            <div class="flex-1 flex items-center justify-center px-6">
              <p class="text-muted-dark text-sm">No note is open.</p>
            </div>
          }
        />
      </main>
      <div data-drawer-part="left">
        <Drawer
          side="left"
          label="Sessions"
          open={openSide() === 'left'}
          onOpenChange={setSide('left')}
          dragPx={leftSwipe.dragPx()}
          width={width()}
        >
          <SessionsPanel />
        </Drawer>
      </div>
      <div data-drawer-part="right">
        <Drawer
          side="right"
          label="Files"
          open={openSide() === 'right'}
          onOpenChange={setSide('right')}
          dragPx={rightSwipe.dragPx()}
          width={width()}
        >
          <FilesPanel />
        </Drawer>
      </div>
    </div>
  );
};
