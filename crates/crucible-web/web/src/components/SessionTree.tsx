import { Component, For, Show, createMemo, createSignal } from 'solid-js';
import { sessionDisplayTitle } from '@/lib/session-display';
import type { Project, Session } from '@/lib/types';
import { Archive, ChevronRight, FlaskConical, FolderGit2, GitBranch, Trash2 } from '@/lib/icons';
import { treeChevron, treeChip, treeGroupRow, treeSectionHeader } from '@/components/tree/tree-style';

export const StateIndicator: Component<{ state: Session['state'] }> = (props) => {
  const colorClass = () => {
    switch (props.state) {
      case 'active': return 'bg-ok';
      case 'paused': return 'bg-attention';
      case 'compacting': return 'bg-primary';
      case 'ended': return 'bg-muted-dark';
      default: return 'bg-muted-dark';
    }
  };

  return (
    <span class={`inline-block w-2 h-2 rounded-full ${colorClass()}`} title={props.state} />
  );
};

function relativeTime(iso: string | null): string | null {
  if (!iso) return null;
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return null;
  const mins = Math.floor((Date.now() - then) / 60_000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

const SessionRow: Component<{
  session: Session;
  selected: boolean;
  branch: string | null;
  kilnLabel: string | null;
  onSelect: () => void;
  onArchive: () => void;
  onDelete: () => void;
}> = (props) => {
  return (
    <div
      onClick={props.onSelect}
      role="button"
      tabindex="0"
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          props.onSelect();
        }
      }}
      class={`group relative w-full text-left pl-6 pr-3 py-1.5 rounded-lg transition-colors cursor-pointer ${
        props.selected
          ? 'bg-primary/20 text-primary-hover'
          : 'hover:bg-hover-wash text-shell-body'
      }`}
      data-testid={`session-item-${props.session.id}`}
    >
      {/* Action-strip padding is reserved only WHILE the buttons show. */}
      <div class="flex items-center gap-2 transition-[padding] duration-150 group-hover:pr-12 group-focus-within:pr-12 [@media(hover:none)]:pr-12">
        <StateIndicator state={props.session.state} />
        <span class="text-[13px] font-medium flex-1 min-w-0 fade-scroll">
          <span>{sessionDisplayTitle(props.session)}</span>
        </span>
        <Show when={relativeTime(props.session.last_activity ?? props.session.started_at)} keyed>
          {(t) => <span class="text-[10px] text-muted-dark shrink-0">{t}</span>}
        </Show>
      </div>
      <div class="flex items-center gap-1 mt-1 min-w-0 transition-[padding] duration-150 group-hover:pr-12 group-focus-within:pr-12 [@media(hover:none)]:pr-12">
        <Show when={props.branch} keyed>
          {(b) => (
            <span class={treeChip} title={`branch · ${b}`}>
              <GitBranch class="w-3 h-3 shrink-0 text-muted-dark" />
              <span class="truncate">{b}</span>
            </span>
          )}
        </Show>
        <Show when={props.kilnLabel} keyed>
          {(k) => (
            <span class={treeChip} title={`kiln · ${props.session.kiln}`}>
              <FlaskConical class="w-3 h-3 shrink-0 text-muted-dark" />
              <span class="truncate">{k}</span>
            </span>
          )}
        </Show>
        <span class="text-[11px] text-muted-dark truncate ml-auto">
          {props.session.agent_model || ''}
        </span>
      </div>

      <div class="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100 transition-opacity duration-150">
        <button
          type="button"
          class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
          title="Archive session"
          onClick={(e) => { e.stopPropagation(); props.onArchive(); }}
        >
          <Archive size={14} />
        </button>
        <button
          type="button"
          class="rounded p-1 text-error/70 hover:text-error hover:bg-error/10 transition-colors"
          title="Delete session"
          onClick={(e) => { e.stopPropagation(); props.onDelete(); }}
        >
          <Trash2 size={14} />
        </button>
      </div>
    </div>
  );
};

interface SessionGroup {
  key: string;
  name: string;
  /** The path selecting this group selects as the current project ('' = none). */
  projectPath: string;
  sessions: Session[];
  lastActivity: number;
}

const NO_PROJECT = '::none';
const COLLAPSED_KEY = 'crucible:sessionTree.collapsed';

function loadCollapsed(): Set<string> {
  try {
    const raw = localStorage.getItem(COLLAPSED_KEY);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}

/**
 * All sessions grouped by project — worktree checkouts fold into their main
 * repo's group (`repository.root`), with the branch shown as a row chip
 * instead of a tree level (branch is a FILTER, not a hierarchy: user call).
 * Groups collapse; recency ordering inside each group and between groups.
 */
export const SessionTree: Component<{
  sessions: Session[];
  currentSessionId?: string;
  projects: Project[];
  currentProjectPath?: string;
  onSelectSession: (id: string) => void;
  onSelectProject: (path: string) => void;
  onArchiveSession: (id: string) => void;
  onDeleteSession: (id: string) => void;
  /** Live branch of a checkout path (from scm.branches), null when unknown. */
  branchOf: (workspace: string) => string | null;
  /** Display name for a kiln path (registered name or basename). */
  kilnName: (path: string) => string | null;
}> = (props) => {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(loadCollapsed(), { equals: false });

  const toggle = (key: string) => {
    setCollapsed((prev) => {
      if (prev.has(key)) prev.delete(key);
      else prev.add(key);
      try {
        localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...prev]));
      } catch {
        /* private mode */
      }
      return prev;
    });
  };

  const groups = createMemo<SessionGroup[]>(() => {
    // checkout path -> group key; group key -> group. Worktrees join their
    // main repo's key so one repo reads as one group.
    const byKey = new Map<string, SessionGroup>();
    const pathToKey = new Map<string, string>();
    for (const p of props.projects) {
      const key = p.repository?.root ?? p.path;
      pathToKey.set(p.path, key);
      const existing = byKey.get(key);
      const isMain = !p.repository?.is_worktree;
      if (!existing) {
        byKey.set(key, {
          key,
          name: isMain ? p.name : (key.split('/').pop() ?? p.name),
          projectPath: p.path,
          sessions: [],
          lastActivity: 0,
        });
      } else if (isMain) {
        // The main checkout names the group and is what selection targets.
        existing.name = p.name;
        existing.projectPath = p.path;
      }
    }

    const none: SessionGroup = {
      key: NO_PROJECT,
      name: 'Session folders',
      projectPath: '',
      sessions: [],
      lastActivity: 0,
    };

    const groupFor = (workspace: string): SessionGroup => {
      const direct = pathToKey.get(workspace);
      if (direct) return byKey.get(direct)!;
      // Longest-prefix fallback: a session started in a repo subdirectory.
      let bestPath: string | null = null;
      let bestKey: string | null = null;
      for (const [path, key] of pathToKey) {
        if (workspace.startsWith(path + '/') && (bestPath === null || path.length > bestPath.length)) {
          bestPath = path;
          bestKey = key;
        }
      }
      return bestKey ? byKey.get(bestKey)! : none;
    };

    for (const s of props.sessions) {
      // Workspace == kiln is the daemon's "no workspace" state.
      const g = s.workspace && s.workspace !== s.kiln ? groupFor(s.workspace) : none;
      g.sessions.push(s);
      const t = Date.parse(s.last_activity ?? s.started_at) || 0;
      if (t > g.lastActivity) g.lastActivity = t;
    }

    const all = [...byKey.values()];
    for (const g of all) {
      g.sessions.sort(
        (a, b) =>
          (Date.parse(b.last_activity ?? b.started_at) || 0) -
          (Date.parse(a.last_activity ?? a.started_at) || 0),
      );
    }
    none.sessions.sort(
      (a, b) =>
        (Date.parse(b.last_activity ?? b.started_at) || 0) -
        (Date.parse(a.last_activity ?? a.started_at) || 0),
    );
    // Active groups by recency, empty project groups after (still clickable
    // for project selection), "No project" last when it has sessions.
    all.sort((a, b) =>
      a.sessions.length && b.sessions.length
        ? b.lastActivity - a.lastActivity
        : b.sessions.length - a.sessions.length || a.name.localeCompare(b.name),
    );
    return none.sessions.length ? [...all, none] : all;
  });

  return (
    <div data-testid="session-list">
      <For each={groups()}>
        {(g) => (
          <div class="mb-1">
            <div class="flex items-center">
              <button
                type="button"
                class={`${treeGroupRow} flex-1 min-w-0`}
                aria-expanded={!collapsed().has(g.key)}
                data-testid={`session-group-${g.name}`}
                onClick={() => toggle(g.key)}
              >
                <ChevronRight
                  class={`${treeChevron} ${collapsed().has(g.key) ? '' : 'rotate-90'}`}
                />
                <FolderGit2
                  classList={{
                    'w-3.5 h-3.5 shrink-0': true,
                    'text-primary': props.currentProjectPath === g.projectPath && !!g.projectPath,
                    'text-muted-dark': props.currentProjectPath !== g.projectPath || !g.projectPath,
                  }}
                />
                <span
                  class="truncate"
                  onClick={(e) => {
                    // Name click selects the project; chevron/row click toggles.
                    if (g.projectPath) {
                      e.stopPropagation();
                      props.onSelectProject(g.projectPath);
                    }
                  }}
                >
                  {g.name}
                </span>
                <span class="text-muted-dark font-normal">{g.sessions.length || ''}</span>
              </button>
            </div>
            <Show when={!collapsed().has(g.key)}>
              <For each={g.sessions}>
                {(s) => (
                  <SessionRow
                    session={s}
                    selected={props.currentSessionId === s.id}
                    branch={
                      s.workspace && s.workspace !== s.kiln ? props.branchOf(s.workspace) : null
                    }
                    kilnLabel={props.kilnName(s.kiln)}
                    onSelect={() => props.onSelectSession(s.id)}
                    onArchive={() => props.onArchiveSession(s.id)}
                    onDelete={() => props.onDeleteSession(s.id)}
                  />
                )}
              </For>
              <Show when={g.sessions.length === 0}>
                <p class={`${treeSectionHeader} normal-case font-normal tracking-normal`}>
                  No sessions
                </p>
              </Show>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
};
