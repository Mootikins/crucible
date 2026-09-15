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
import { EmptyState } from '@/components/ui/EmptyState';
import { TabOverview } from '@/components/mobile/TabOverview';
import { MobileEditorBar } from '@/components/mobile/MobileEditorBar';
import { OfflineBadge } from '@/components/OfflineBadge';
import { BottomSheet, SheetOption } from '@/components/mobile/BottomSheet';
import { openPanelTab } from '@/lib/panel-actions';
import { conflictActions, conflictStore, openConflict } from '@/lib/conflicts';
import { iconForPanelId } from '@/lib/tab-icons';
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
  // Conflicts has its own row above the list, with its count. A second,
  // countless door to the same tab would read as a different surface.
  'conflicts',
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
  const conflicts = () => conflictStore.count();
  // A phone has no rail to park a count in, so this menu is where a conflict
  // announces itself. Read again as the sheet opens: a drain while the user
  // was reading is exactly when one appears.
  const readConflicts = () => void conflictActions.refresh().catch(() => undefined);
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
    readConflicts();
    const onResize = () => setViewport(window.innerWidth);
    window.addEventListener('resize', onResize);
    onCleanup(() => window.removeEventListener('resize', onResize));
  });

  // One drawer at a time: opening a side replaces whatever was open.
  const setSide = (side: DrawerSide) => (open: boolean) =>
    setOpenSide(open ? side : openSide() === side ? null : openSide());

  /**
   * Open the left drawer on its Files tab.
   *
   * `DrawerTabs` owns which tab shows, and both tabs stay mounted, so the
   * button is already in the DOM and a click on it is the whole selection.
   * A second copy of that state in this shell would disagree with the strip
   * as soon as the user touched a tab.
   */
  const openFilesDrawer = () => {
    setSide('left')(true);
    document.getElementById('drawer-tab-files')?.click();
  };

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
        // 56 px, not 48: the controls inside are 44 px touch targets, and a
        // filled one (Read/Write) in a 48 px bar leaves 2 px of clearance, so
        // its background reads as touching the bar's edges.
        class="shrink-0 flex items-center gap-1 h-14 px-2 border-b border-hairline bg-surface-elevated"
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
          onClick={() => {
            readConflicts();
            setMenuOpen(true);
          }}
        >
          <MoreHorizontal class="w-5 h-5" />
        </button>
        <DrawerButton side="right" label="Backlinks" icon={Link2} />
      </header>
      <main
        class="flex-1 min-h-0 flex flex-col"
        // Thumb metrics for everything the pane renders: the properties card
        // header, and the rows of any tree a panel tab opens here. A tree
        // states its own density only when a caller passes one.
        data-density="touch"
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
            <EmptyState
              class="flex-1"
              title="No note is open"
              body="Open one from the files drawer, or start a session."
              action={[
                { label: 'Open a note', onClick: openFilesDrawer },
                {
                  label: 'Start a session',
                  onClick: () => window.dispatchEvent(new CustomEvent('crucible:new-session')),
                },
              ]}
              testid="mobile-empty"
            />
          }
        />
        </Show>
      </main>
      <BottomSheet open={menuOpen()} label="More" onClose={() => setMenuOpen(false)}>
        {/* Above the panel list, and only when one waits: a conflict is the
            one thing here that is owed to the user rather than offered to
            them, and nothing else on a phone says so. */}
        <Show when={conflicts() > 0}>
          <SheetOption
            label={`Conflicts (${conflicts()})`}
            onSelect={() => {
              setMenuOpen(false);
              openConflict();
            }}
          />
        </Show>
        <For each={menuPanels()}>
          {(def) => (
            <SheetOption
              label={def.title}
              icon={iconForPanelId(def.id)}
              onSelect={() => {
                setMenuOpen(false);
                openPanelTab(def.id as Parameters<typeof openPanelTab>[0]);
              }}
            />
          )}
        </For>
        {/* Its own row, under the panels, because settings is a DIALOG and the
            registry holds only panels. The phone opens the same drill-down the
            desktop gear opens. */}
        <SheetOption
          label="Settings"
          icon={iconForPanelId('settings')}
          onSelect={() => {
            setMenuOpen(false);
            window.dispatchEvent(new CustomEvent('crucible:open-settings'));
          }}
        />
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
              { id: 'files', label: 'Files', content: () => <FilesPanel density="touch" /> },
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
