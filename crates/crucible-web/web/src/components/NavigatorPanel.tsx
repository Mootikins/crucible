import { Component, For, Show, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
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
import { ChevronDown, Check, Search, X, ClipboardList, FlaskConical, FolderGit2 } from '@/lib/icons';

type Mode = 'files' | 'sessions';

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

  onMount(() => swrLocal('kilns', listKilns, setKilns));

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
    const close = (e: MouseEvent) => { if (!(e.target as HTMLElement).closest('[data-nav-swapper]')) setOpen(false); };
    document.addEventListener('click', close);
    // Ctrl+Shift+F / palette open us in search mode.
    const onFocusSearch = () => setSearching(true);
    window.addEventListener('crucible:focus-search', onFocusSearch);
    onCleanup(() => { document.removeEventListener('click', close); window.removeEventListener('crucible:focus-search', onFocusSearch); });
  });

  const groupsNonEmpty = () => roster().filter((g) => g.roots.length > 0);
  const isRootActive = (r: TreeRoot) => mode() === 'files' && activeRoot() && rootKey(activeRoot()!) === rootKey(r);

  return (
    <PanelShell class="overflow-hidden">
      {/* Header: scope swapper + search toggle */}
      <div class="flex items-center gap-1 px-2 py-2 border-b border-hairline shrink-0">
        <div class="relative flex-1 min-w-0" data-nav-swapper>
          <button
            type="button"
            onClick={() => setOpen((o) => !o)}
            class="w-full flex items-center gap-1.5 h-8 px-2 rounded-md hover:bg-hover-wash text-[13px] text-shell-ink"
            data-testid="navigator-swapper"
          >
            {(() => { const I = ScopeIcon(); return <I class="w-4 h-4 shrink-0 text-muted-dark" />; })()}
            <span class="font-medium truncate">{scopeLabel()}</span>
            <ChevronDown class="w-3.5 h-3.5 shrink-0 text-muted-dark ml-auto" />
          </button>
          <Show when={open()}>
            <div class="absolute left-0 top-9 z-20 w-64 max-h-[70vh] overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1">
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
