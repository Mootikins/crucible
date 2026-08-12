import { Component, For, Show, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';
import { PanelShell } from './PanelShell';
import { FilesPanel } from './FilesPanel';
import { SearchPanel } from './SearchPanel';
import { SessionsBody } from './navigator/SessionsBody';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { listKilns } from '@/lib/api';
import type { KilnListEntry } from '@/lib/types';
import { swrLocal } from '@/lib/local-cache';
import { buildRoster, rosterIndex, rootKey, type RosterGroup, type TreeRoot } from '@/lib/tree-root';
import { selectedRootKey, treeRootActions } from '@/stores/treeRootStore';
import { pathBasename } from '@/stores/statusBarStore';
import { placePopup, type PopupPlacement } from '@/lib/popup-placement';
import { ChevronDown, Check, Search, X, ClipboardList, FlaskConical, FolderGit2 } from '@/lib/icons';

type Mode = 'files' | 'sessions';

/** Scope menu width (was `w-64`) and the height at which its list scrolls. */
const SCOPE_MENU_WIDTH = 256;
const SCOPE_MENU_MAX_HEIGHT = 340;
/** Gap between the swapper trigger and the menu. */
const SCOPE_MENU_GAP = 4;

const rootName = (r: TreeRoot) => r.name || pathBasename(r.path) || r.path;
const rootIcon = (kind: TreeRoot['kind']) => (kind === 'project' ? FolderGit2 : FlaskConical);

/**
 * The unified left navigator (replaces the separate Files / Sessions / Search
 * tabs). A scope swapper switches the body between a kiln/project file tree and
 * the sessions list; a search button takes the body over. The tree, sessions,
 * and search engines are the existing components, composed here.
 */
export const NavigatorPanel: Component = () => {
  const { projects } = useProjectSafe();
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [mode, setMode] = createSignal<Mode>('files');
  const [searching, setSearching] = createSignal(false);
  const [open, setOpen] = createSignal(false);
  const [menuPos, setMenuPos] = createSignal<PopupPlacement | null>(null);
  let triggerRef: HTMLButtonElement | undefined;
  let menuRef: HTMLDivElement | undefined;

  onMount(() => swrLocal('kilns', listKilns, setKilns));

  /**
   * Place the scope menu against the trigger, in viewport coordinates.
   *
   * The menu is a fixed-size card (no measuring pass needed): width is the old
   * `w-64` and the list scrolls past `SCOPE_MENU_MAX_HEIGHT`, so `placePopup`
   * can decide up/down and clamp horizontally from those alone.
   */
  const positionMenu = () => {
    if (!triggerRef) return;
    setMenuPos(
      placePopup(
        triggerRef.getBoundingClientRect(),
        { width: window.innerWidth, height: window.innerHeight },
        { width: SCOPE_MENU_WIDTH, preferredHeight: SCOPE_MENU_MAX_HEIGHT, gap: SCOPE_MENU_GAP },
      ),
    );
  };

  const toggleMenu = () => {
    if (open()) {
      setOpen(false);
      return;
    }
    positionMenu();
    setOpen(true);
  };

  const roster = createMemo<RosterGroup[]>(() => buildRoster(projects(), kilns()));
  const activeRoot = createMemo<TreeRoot | null>(() => {
    const groups = roster();
    const idx = rosterIndex(groups);
    const persisted = selectedRootKey();
    if (persisted && idx.has(persisted)) return idx.get(persisted)!;
    const firstProject = groups.find((g) => g.kind === 'project' && g.roots.length > 0)?.roots[0];
    const firstKiln = groups.find((g) => g.kind === 'kiln' && g.roots.length > 0)?.roots[0];
    return firstProject ?? firstKiln ?? null;
  });

  const pickRoot = (r: TreeRoot) => { treeRootActions.selectRoot(r); setMode('files'); setSearching(false); setOpen(false); };
  const pickSessions = () => { setMode('sessions'); setSearching(false); setOpen(false); };

  // Swapper trigger label/icon reflects the current scope.
  const scopeLabel = () => (mode() === 'sessions' ? 'Sessions' : activeRoot() ? rootName(activeRoot()!) : 'No root');
  const ScopeIcon = () => (mode() === 'sessions' ? ClipboardList : activeRoot() ? rootIcon(activeRoot()!.kind) : FlaskConical);

  onMount(() => {
    // The menu is portaled out of `[data-nav-swapper]`, so containment alone no
    // longer says "inside the control" — check the menu element too, or the
    // first click on any of its own rows reads as an outside click.
    const close = (e: MouseEvent) => {
      const t = e.target as Node;
      if ((t as HTMLElement).closest?.('[data-nav-swapper]')) return;
      if (menuRef?.contains(t)) return;
      setOpen(false);
    };
    document.addEventListener('click', close);
    // Fixed coordinates go stale when the viewport moves under them.
    const reposition = () => { if (open()) positionMenu(); };
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    // Ctrl+Shift+F / palette open us in search mode.
    const onFocusSearch = () => setSearching(true);
    window.addEventListener('crucible:focus-search', onFocusSearch);
    onCleanup(() => {
      document.removeEventListener('click', close);
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
      window.removeEventListener('crucible:focus-search', onFocusSearch);
    });
  });

  const groupsNonEmpty = () => roster().filter((g) => g.roots.length > 0);
  const isRootActive = (r: TreeRoot) => mode() === 'files' && activeRoot() && rootKey(activeRoot()!) === rootKey(r);

  return (
    <PanelShell class="overflow-hidden">
      {/* Header: scope swapper + search toggle */}
      <div class="flex items-center gap-1 px-2 py-2 border-b border-hairline shrink-0">
        {/* No `relative` — the menu is portaled and viewport-positioned. */}
        <div class="flex-1 min-w-0" data-nav-swapper>
          <button
            ref={triggerRef}
            type="button"
            onClick={toggleMenu}
            class="w-full flex items-center gap-1.5 h-8 px-2 rounded-md hover:bg-hover-wash text-[13px] text-shell-ink"
            data-testid="navigator-swapper"
          >
            {(() => { const I = ScopeIcon(); return <I class="w-4 h-4 shrink-0 text-muted-dark" />; })()}
            <span class="font-medium truncate">{scopeLabel()}</span>
            <ChevronDown class="w-3.5 h-3.5 shrink-0 text-muted-dark ml-auto" />
          </button>
          {/* Portaled, fixed-positioned: the Navigator lives in an EdgePanel
              whose slide frame is `overflow-hidden` and whose inner wrapper
              always carries a `translate` — a stacking context AND a
              containing block. An in-flow `absolute` menu is therefore clipped
              at the panel's right edge and painted beneath the center pane, and
              no z-index can lift it out. Same escape hatch as ChipSelect. */}
          <Show when={open()}>
            <Portal>
              <div
                ref={menuRef}
                data-testid="navigator-scope-menu"
                class="z-50 overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1"
                style={{
                  // Inline, not a `fixed` utility class: the geometry below is
                  // inline anyway, so one declaration owns the positioning.
                  position: 'fixed',
                  left: `${menuPos()?.left ?? 0}px`,
                  ...(menuPos()?.bottom !== undefined
                    ? { bottom: `${menuPos()!.bottom}px` }
                    : { top: `${menuPos()?.top ?? 0}px` }),
                  width: `${SCOPE_MENU_WIDTH}px`,
                  'max-height': `${menuPos()?.maxHeight ?? SCOPE_MENU_MAX_HEIGHT}px`,
                }}
              >
                {/* Sessions: top entry, no header line */}
                <button
                  type="button"
                  onClick={pickSessions}
                  class="w-full flex items-center gap-2 px-3 py-1.5 text-[13px] text-shell-body hover:bg-hover-wash"
                  data-testid="navigator-scope-sessions"
                >
                  <ClipboardList class="w-4 h-4 shrink-0 text-muted-dark" />
                  <span class="truncate">Sessions</span>
                  <Show when={mode() === 'sessions'}><Check class="w-3.5 h-3.5 text-primary ml-auto" /></Show>
                </button>
                <div class="my-1 border-t border-hairline" />
                <For each={groupsNonEmpty()}>
                  {(g) => (
                    <>
                      <div class="px-3 pt-1.5 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-dark">{g.label}</div>
                      <For each={g.roots}>
                        {(r) => {
                          const I = rootIcon(r.kind);
                          return (
                            <button
                              type="button"
                              onClick={() => pickRoot(r)}
                              class="w-full flex items-center gap-2 px-3 py-1.5 text-[13px] text-shell-body hover:bg-hover-wash"
                            >
                              <I class="w-4 h-4 shrink-0 text-muted-dark" />
                              <span class="truncate">{rootName(r)}</span>
                              <Show when={isRootActive(r)}><Check class="w-3.5 h-3.5 text-primary ml-auto" /></Show>
                            </button>
                          );
                        }}
                      </For>
                    </>
                  )}
                </For>
              </div>
            </Portal>
          </Show>
        </div>
        <button
          type="button"
          title="Search"
          onClick={() => setSearching((s) => !s)}
          classList={{
            'w-8 h-8 rounded-md flex items-center justify-center shrink-0 transition-colors': true,
            'bg-primary/15 text-primary': searching(),
            'text-muted-dark hover:bg-hover-wash hover:text-shell-ink': !searching(),
          }}
          data-testid="navigator-search-toggle"
        >
          <Show when={searching()} fallback={<Search class="w-4 h-4" />}><X class="w-4 h-4" /></Show>
        </button>
      </div>

      {/* Body — the search takeover, the sessions list, or the file tree. */}
      <div class="flex-1 min-h-0 overflow-hidden">
        <Show when={searching()} fallback={
          <Show when={mode() === 'sessions'} fallback={<FilesPanel embedded />}>
            <SessionsBody />
          </Show>
        }>
          <SearchPanel />
        </Show>
      </div>
    </PanelShell>
  );
};
