import { Component, For, Show, createEffect, createSignal, on, onCleanup, onMount } from 'solid-js';
import { ContentSurface } from '@/components/mobile/ContentSurface';
import { Drawer } from '@/components/mobile/Drawer';
import { createEdgeSwipe, type SwipePoint } from '@/components/mobile/edge-swipe';
import type { DrawerSide } from '@/components/mobile/drawer-gesture';
import { SessionsTab } from '@/components/mobile/SessionsTab';
import { FilesPanel } from '@/components/FilesPanel';
import { BacklinksPanel } from '@/components/BacklinksPanel';
import { DrawerTabs } from '@/components/mobile/DrawerTabs';
import { FolderTree, Link2, MoreHorizontal } from '@/lib/icons';
import { TabOverview } from '@/components/mobile/TabOverview';
import { MobileEditorBar } from '@/components/mobile/MobileEditorBar';
import { OfflineBadge } from '@/components/mobile/OfflineBadge';
import { BottomSheet, SheetOption } from '@/components/mobile/BottomSheet';
import { openPanelTab } from '@/lib/panel-actions';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { navStack } from '@/components/mobile/NavStack';
import { tabStack, tabStackActions } from '@/stores/tabStackStore';
import { LayoutDashboard } from '@/lib/icons';

/**
 * Panels the overflow menu does NOT offer.
 *
 * The two drawers already hold sessions, files and backlinks. The rest are
 * either undrawable on a phone — a terminal needs a keyboard, a canvas needs
 * drag and a large field — or they open with a target rather than from a menu
 * (a file, a chat, the draft).
 */
const NOT_IN_MENU = new Set([
  'sessions',
  'files',
  'backlinks',
  'terminal',
  'canvas',
  'file',
  'chat',
  'chat-draft',
]);

/** `min(85vw, 320px)`, in px, because the swipe measures against it. */
const drawerWidthFor = (viewport: number) => Math.min(Math.round(viewport * 0.85), 320);

/**
 * The compact shell: an app bar over one content surface, with an edge drawer
 * on each side. The left drawer is where a user goes — sessions and files, as
 * tabs. The right drawer is the open note's context — its backlinks. Decision
 * log 2026-09-11; see `docs/Meta/Architecture/Mobile Shell.md`.
 *
 * The tab stack and the drawers' own tabs arrive in later steps of Track A.
 */
export const MobileShell: Component = () => {
  const activeTab = () => tabStackActions.activeTab();
  const [overviewOpen, setOverviewOpen] = createSignal(false);
  const [menuOpen, setMenuOpen] = createSignal(false);
  const menuPanels = () =>
    getGlobalRegistry()
      .list()
      .filter((def) => !NOT_IN_MENU.has(def.id))
      .sort((a, b) => a.title.localeCompare(b.title));

  // Each move to a tab gets a history entry, so the phone's back button walks
  // the tabs a user has seen before it leaves the app. `back()` answers false
  // once every tab has been walked; the entry is spent either way, so the next
  // press belongs to the browser.
  let movingBack = false;
  createEffect(
    on(
      () => activeTab()?.id,
      (id, previous) => {
        if (!id || id === previous || movingBack) return;
        navStack().push(() => {
          movingBack = true;
          tabStackActions.back();
          movingBack = false;
        });
      },
    ),
  );

  const openOverview = () => {
    setOverviewOpen(true);
    const release = navStack().push(() => setOverviewOpen(false));
    releaseOverview = () => {
      release();
      releaseOverview = null;
    };
  };
  let releaseOverview: (() => void) | null = null;
  const closeOverview = () => {
    setOverviewOpen(false);
    releaseOverview?.();
  };
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
        class="w-11 h-11 flex items-center justify-center shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
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
        class="shrink-0 flex items-center gap-1 h-12 px-1 border-b border-hairline bg-surface-elevated"
        style={{ 'padding-top': 'var(--inset-top)', 'box-sizing': 'content-box' }}
      >
        <DrawerButton side="left" label="Sessions and files" icon={FolderTree} />
        <h1 class="text-reading flex-1 truncate font-medium text-shell-ink px-1">
          {activeTab()?.title ?? 'Crucible'}
        </h1>
        <OfflineBadge />
        <Show when={activeTab()?.contentType === 'file' && !overviewOpen()}>
          <MobileEditorBar filePath={String(activeTab()!.metadata?.filePath ?? '')} />
        </Show>
        <Show when={tabStack.tabs.length > 0}>
          <button
            type="button"
            aria-label={`Tabs (${tabStack.tabs.length})`}
            class="w-11 h-11 flex items-center justify-center shrink-0 gap-1 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
            onClick={() => (overviewOpen() ? closeOverview() : openOverview())}
          >
            <LayoutDashboard class="w-5 h-5" />
            <span class="text-floor tabular-nums">{tabStack.tabs.length}</span>
          </button>
        </Show>
        <button
          type="button"
          aria-label="More"
          class="w-11 h-11 flex items-center justify-center shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
          onClick={() => setMenuOpen(true)}
        >
          <MoreHorizontal class="w-5 h-5" />
        </button>
        <DrawerButton side="right" label="Backlinks" icon={Link2} />
      </header>
      <main
        class="flex-1 min-h-0 flex flex-col"
        // The browser keeps vertical scroll; horizontal travel reaches the swipe.
        style={{ 'padding-bottom': 'var(--inset-bottom)', 'touch-action': 'pan-y' }}
      >
        <Show
          when={!overviewOpen()}
          fallback={
            <TabOverview
              tabs={tabStack.tabs}
              activeId={tabStack.activeTabId}
              onPick={(id) => {
                tabStackActions.activate(id);
                closeOverview();
              }}
              onClose={(id) => tabStackActions.remove(id)}
            />
          }
        >
        <ContentSurface
          tab={activeTab}
          empty={
            <div class="flex-1 flex items-center justify-center px-6">
              <p class="text-reading text-muted-dark">No note is open.</p>
            </div>
          }
        />
        </Show>
      </main>
      <BottomSheet open={menuOpen()} label="More" onClose={() => setMenuOpen(false)}>
        <For each={menuPanels()}>
          {(def) => (
            <SheetOption
              label={def.title}
              onSelect={() => {
                setMenuOpen(false);
                openPanelTab(def.id as Parameters<typeof openPanelTab>[0]);
              }}
            />
          )}
        </For>
      </BottomSheet>
      <div data-drawer-part="left">
        <Drawer
          side="left"
          label="Sessions and files"
          open={openSide() === 'left'}
          onOpenChange={setSide('left')}
          dragPx={leftSwipe.dragPx()}
          width={width()}
        >
          <DrawerTabs
            label="Sessions and files"
            tabs={[
              { id: 'sessions', label: 'Sessions', content: () => <SessionsTab /> },
              { id: 'files', label: 'Files', content: () => <FilesPanel /> },
            ]}
          />
        </Drawer>
      </div>
      <div data-drawer-part="right">
        <Drawer
          side="right"
          label="Backlinks"
          open={openSide() === 'right'}
          onOpenChange={setSide('right')}
          dragPx={rightSwipe.dragPx()}
          width={width()}
        >
          <BacklinksPanel />
        </Drawer>
      </div>
    </div>
  );
};
