import { Component, For, JSX, Show, createMemo, createSignal, onCleanup } from 'solid-js';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { menuContent, menuItem } from '@/components/ui/menu-style';
import { sessionDisplayTitle } from '@/lib/session-display';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
import type { Project, Session } from '@/lib/types';
import { Archive, ChevronRight, GitBranch, MessageCircle, Pin, Plus, Trash2 } from '@/lib/icons';
import { treeChevron, treeGroupRow } from '@/components/tree/tree-style';
import { TreeSection } from '@/components/tree/TreeSection';
import { shouldUseNativeMenu } from '@/windowing';
import { terseAge } from '@/lib/format-time';
import { sessionStatus } from '@/lib/session-status';
import { byRecency, touchedAt } from '@/lib/session-inbox';
import { SessionStatusDot } from '@/components/shell/SessionStatusDot';

export const SessionRow: Component<{
  session: Session;
  selected: boolean;
  branch: string | null;
  kilnLabel: string | null;
  /** True when this row's kiln differs from its group's usual one — see
   * `oddKiln`. A kiln every sibling shares distinguishes nothing. */
  showKiln?: boolean;
  /**
   * The project's name, for a row drawn OUTSIDE its project group — the
   * Inbox. Leaving the group is exactly when a row loses its project, so
   * that is the row that carries it; under a header it would repeat the
   * header. Such a row is not indented either: it has no parent to sit under.
   */
  projectLabel?: string | null;
  onSelect: () => void;
  onArchive: () => void;
  onDelete: () => void;
}> = (props) => {
  const age = () => terseAge(props.session.last_activity ?? props.session.started_at);
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
      title={props.session.kilns.length ? `kilns · ${props.session.kilns.join(', ')}` : undefined}
      /*
       * ONE line, ONE height, indented under its project.
       *
       * It used to be two lines whose second held the kiln name, which put the
       * quietest text in the panel FURTHEST LEFT — left of the project it
       * belongs to — so the tier read backwards and the rail scanned as a flat
       * list. It also made rows 50px with a kiln and 32px without, so the
       * column had no beat to chunk on. Indent carries the tier here, the way
       * every file explorer does it.
       */
      class={`group relative flex items-center gap-2 w-full h-(--cru-row-sm) ${
        props.projectLabel === undefined ? 'pl-6' : 'pl-2'
      } pr-2 rounded transition-colors cursor-pointer ${
        props.selected
          ? 'bg-primary/10 text-shell-ink'
          : 'hover:bg-hover-wash text-shell-body'
      }`}
      data-testid={`session-item-${props.session.session_id}`}
      data-session-id={props.session.session_id}
    >
      {/* The same 14px leading slot a section's chevron sits in. */}
      <span class="w-3.5 shrink-0 flex justify-center" aria-hidden="true">
        <SessionStatusDot status={sessionStatus(props.session)} />
      </span>
      <span class="text-reading flex-1 min-w-0 truncate">{sessionDisplayTitle(props.session)}</span>

      {/* Plain muted text, not a chip: the branch chip below is already the
          one box on the row, and the project is a place, not a state. */}
      <Show when={props.projectLabel} keyed>
        {(name) => (
          <span class="shrink-0 truncate max-w-[96px] text-floor text-muted-dark" title={`project · ${name}`}>
            {name}
          </span>
        )}
      </Show>

      {/* Only when it says something the group row does not. */}
      <Show when={props.showKiln && props.kilnLabel} keyed>
        {(k) => (
          <span class="shrink-0 truncate max-w-[80px] text-floor text-muted-dark">{k}</span>
        )}
      </Show>
      <Show when={props.branch} keyed>
        {(b) => (
          <span
            class="shrink-0 inline-flex items-center gap-1 px-1 rounded bg-surface-elevated border border-hairline text-floor text-muted-dark"
            title={`branch · ${b}`}
          >
            <GitBranch class="w-2.5 h-2.5 shrink-0" />
            <span class="truncate max-w-[80px]">{b}</span>
          </span>
        )}
      </Show>

      {/* The age column keeps its width whether or not the action shows, so a
          hover does not shove the row's own contents sideways. */}
      <span class="w-8 shrink-0 text-right text-floor text-muted-dark group-hover:invisible group-focus-within:invisible">
        {age() ?? ''}
      </span>

      <div class="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100 transition-opacity duration-150">
        {/* Archive only. Delete is destructive and lived 4px away, red, on
            every row — it belongs behind the context menu, where every file
            explorer keeps it. */}
        <button
          type="button"
          class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
          title="Archive session"
          aria-label={`Archive ${sessionDisplayTitle(props.session)}`}
          onClick={(e) => { e.stopPropagation(); props.onArchive(); }}
        >
          <Archive size={14} />
        </button>
      </div>
    </div>
  );
};

const GROUP_ITEMS: MenuItem[] = [
  { value: 'new-session', label: 'New session here', icon: Plus },
  { value: 'pin-project', label: 'Pin project', icon: Pin },
];
const SESSION_ITEMS: MenuItem[] = [
  { value: 'open-session', label: 'Open', icon: MessageCircle },
  { value: 'archive-session', label: 'Archive', icon: Archive },
  { value: 'delete-session', label: 'Delete', icon: Trash2, danger: true },
];

interface MenuItem {
  value: string;
  label: string;
  icon: Component<{ class?: string }>;
  danger?: boolean;
}

/** What the one hoisted context menu currently acts on. */
type MenuTarget =
  | { kind: 'group'; group: SessionGroup }
  | { kind: 'session'; session: Session };

interface SessionGroup {
  key: string;
  name: string;
  /** The path selecting this group selects as the current project ('' = none). */
  projectPath: string;
  /** Every session of the group, Inbox members included. */
  sessions: Session[];
  /** The rows this tree draws: `sessions` minus what the Inbox shows above. */
  rows: Session[];
  lastActivity: number;
}

const NO_PROJECT = '::none';

/** Paths compare trimmed everywhere — see `sessionInProject`. */
function trimSlash(p: string): string {
  return p.replace(/\/+$/, '');
}
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
 *
 * A project with no session is not drawn. It used to sit behind a counted
 * "No sessions" fold, but a detected directory is not a place the user works
 * yet, and a registry of twenty of them is mostly noise. It appears the
 * moment a session starts in it.
 */
export const SessionTree: Component<{
  sessions: Session[];
  /**
   * The Inbox: the sessions drawn ABOVE the project tier, flat, freshest
   * first — see `lib/session-inbox.ts`. The tree draws them here, not the
   * panel, so the one context menu and the one click handler reach an inbox
   * row as they reach a tree row. The tier below does not repeat them: a row
   * drawn twice on one rail is one row too many. Their project still shows,
   * so New Session stays reachable there — without a chevron when nothing is
   * left under it to unfold.
   */
  inbox?: Session[];
  /** Controls for the Projects section header: the project menu. */
  projectsActions?: JSX.Element;
  currentSessionId?: string;
  projects: Project[];
  currentProjectPath?: string;
  onSelectSession: (id: string) => void;
  onSelectProject: (path: string) => void;
  /**
   * Start a session in this project. Required, not optional: a project tier
   * you cannot start work from is a filing cabinet, and the panel-wide "New
   * Session" button it replaces could not say which project it meant.
   */
  onNewSession: (projectPath: string) => void;
  onArchiveSession: (id: string) => void;
  onDeleteSession: (id: string) => void;
  /** Live branch of a checkout path (from the workspace provider), null when unknown. */
  branchOf: (workspace: string) => string | null;
  /** Display name for a kiln path (registered name or basename). */
  kilnName: (path: string) => string | null;
}> = (props) => {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(loadCollapsed(), { equals: false });
  const [inboxOpen, setInboxOpen] = createSignal(true);
  const inbox = () => props.inbox ?? [];
  const shownAbove = createMemo(() => new Set(inbox().map((s) => s.session_id)));
  const waitingCount = () => inbox().filter((s) => sessionStatus(s) === 'waiting').length;
  /**
   * The project row the open context menu acts on.
   *
   * ONE trigger for the whole tree, resolved from the event target — the same
   * choice `FileTreeContextMenu` makes. A trigger per row costs a zag machine
   * and a portalled div per row to hold a value one signal holds.
   */
  const [menuTarget, setMenuTarget] = createSignal<MenuTarget | null>(null);
  const [idleOpen, setIdleOpen] = createSignal(false);
  const [projectsOpenRaw, setProjectsOpen] = createSignal(true);

  // A kiln-less session is a legitimate shape, so its row says nothing about
  // kilns. `kilnName` is never handed '' to resolve: the empty path is a real
  // directory to every label helper (the home data dir), so resolving it would
  // print an attachment the session does not have.
  const kilnNameOf = (s: Session): string | null => {
    const kiln = sessionDefaultKiln(s);
    return kiln ? props.kilnName(kiln) : null;
  };

  /** Same shape for the workspace: a session with none has no branch. */
  const branchOfSession = (s: Session): string | null => {
    const workspace = sessionWorkspace(s);
    return workspace ? props.branchOf(workspace) : null;
  };

  /**
   * A group that holds the open session is never collapsed, whatever the
   * saved state says. Selected from the palette, a session inside a group
   * collapsed last week was on screen nowhere. The saved state stays as it
   * was; the exception lasts as long as the selection does.
   */
  const isCollapsed = (g: SessionGroup) =>
    collapsed().has(g.key) && !g.rows.some((s) => s.session_id === props.currentSessionId);

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

  /**
   * The kiln most of a group's sessions share.
   *
   * A kiln every sibling has distinguishes nothing — the rail rendered
   * "docs"/"crucible-kiln" down a whole column and said the same thing on
   * every row. Only the odd one out earns the pixels. Counted over the rows
   * drawn here, since those are the siblings the eye compares.
   */
  const dominantKiln = (g: SessionGroup): string | null => {
    const counts = new Map<string, number>();
    for (const s of g.rows) {
      const kiln = kilnNameOf(s);
      if (kiln) counts.set(kiln, (counts.get(kiln) ?? 0) + 1);
    }
    let best: string | null = null;
    let bestCount = 0;
    for (const [kiln, count] of counts) {
      if (count > bestCount) {
        best = kiln;
        bestCount = count;
      }
    }
    return best;
  };

  const allGroups = createMemo<SessionGroup[]>(() => {
    // checkout path -> group key; group key -> group. Worktrees join their
    // main repo's key so one repo reads as one group.
    const byKey = new Map<string, SessionGroup>();
    const pathToKey = new Map<string, string>();
    for (const p of props.projects) {
      // Trimmed, like `sessionInProject`. Untrimmed, a project registered as
      // `/repo/` matched nothing here while the switcher matched it fine, so
      // one session sat in two different projects depending on the panel.
      const key = trimSlash(p.repository?.root ?? p.path);
      pathToKey.set(trimSlash(p.path), key);
      const existing = byKey.get(key);
      const isMain = !p.repository?.is_worktree;
      if (!existing) {
        byKey.set(key, {
          key,
          name: isMain ? p.name : (key.split('/').pop() ?? p.name),
          projectPath: p.path,
          sessions: [],
          rows: [],
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
      rows: [],
      lastActivity: 0,
    };

    const groupFor = (rawWorkspace: string): SessionGroup => {
      const workspace = trimSlash(rawWorkspace);
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
      const workspace = sessionWorkspace(s);
      const g = workspace ? groupFor(workspace) : none;
      g.sessions.push(s);
      g.lastActivity = Math.max(g.lastActivity, touchedAt(s));
    }

    const all = [...byKey.values(), none];
    for (const g of all) {
      g.sessions.sort(byRecency);
      g.rows = g.sessions.filter((s) => !shownAbove().has(s.session_id));
    }
    // Every registered project is listed, with or without a session. The
    // project-less group has no New Session of its own, so it draws only
    // with a row to unfold.
    return all.filter((g) => g.rows.length > 0 || !!g.projectPath);
  });

  /** The project an inbox row names: the group its workspace falls in. */
  const projectLabelOf = (s: Session): string | null => {
    const g = allGroups().find((x) => x.sessions.some((m) => m.session_id === s.session_id));
    return g?.projectPath ? g.name : null;
  };

  const live = createMemo<SessionGroup[]>(() =>
    [...allGroups()].sort(
      (a, b) => b.lastActivity - a.lastActivity || a.name.localeCompare(b.name),
    ),
  );

  /** True when the pinned project actually has a group to scope to. */
  const scoped = () =>
    !!props.currentProjectPath && live().some((g) => g.projectPath === props.currentProjectPath);

  /**
   * The tree's body: the PINNED project when there is one, everything
   * otherwise.
   *
   * Scoping to a pin that matches nothing would empty the rail, which is a
   * dead end on the screen a new user starts from — so the scope only applies
   * when it has something to show.
   */
  const groups = createMemo<SessionGroup[]>(() =>
    scoped() ? live().filter((g) => g.projectPath === props.currentProjectPath) : live(),
  );

  /**
   * Everything the pin left out, folded and COUNTED: the other projects that
   * have sessions.
   *
   * A filter you cannot see is worse than the rows it saves, so the count
   * states how much is hidden and one click shows it. They stay fully usable
   * when open: a folded project is the same header row as a pinned one, so
   * New Session and the context menu work there too.
   */
  const offScope = createMemo<SessionGroup[]>(() => {
    const shown = new Set(groups().map((g) => g.key));
    return live().filter((g) => !shown.has(g.key));
  });

  /** The section, like a group, does not hide the open session. */
  const projectsOpen = () =>
    projectsOpenRaw() ||
    groups().some((g) => g.rows.some((s) => s.session_id === props.currentSessionId));

  /** The fold, like a group, does not hide the open session. */
  const foldOpen = () =>
    idleOpen() ||
    offScope().some((g) => g.rows.some((s) => s.session_id === props.currentSessionId));

  /**
   * Capture-phase router for the single hoisted context trigger.
   *
   * It listens on a wrapper OUTSIDE the trigger, so it always runs first and
   * can both resolve the row and VETO the open by stopping propagation. It
   * cannot live on the trigger element: `triggerProps` already spreads an
   * `onContextMenu`, and a second one on the same element replaces it — the
   * menu then never opens at all.
   *
   * Vetoed: the shared native-menu rule, and anything that hits neither a
   * session row nor a project row with a project. Non-mouse pointerdown is
   * zag's long-press path and routes the same.
   */
  const attachContextRouter = (el: HTMLElement) => {
    const route = (e: Event): boolean => {
      const target = e.target instanceof Element ? e.target : null;
      const sessionId = target?.closest('[data-session-id]')?.getAttribute('data-session-id');
      if (sessionId) {
        const session = props.sessions.find((x) => x.session_id === sessionId);
        if (session) {
          setMenuTarget({ kind: 'session', session });
          return true;
        }
      }
      const key = target?.closest('[data-group-key]')?.getAttribute('data-group-key');
      // ALL groups, not just the pinned ones: a project in the folded "Other
      // projects" section renders the same header row, and searching only
      // the scoped list vetoed its menu.
      const g = allGroups().find((x) => x.key === key) ?? null;
      setMenuTarget(g?.projectPath ? { kind: 'group', group: g } : null);
      return !!g?.projectPath;
    };
    const onContextMenu = (e: MouseEvent) => {
      if (shouldUseNativeMenu(e) || !route(e)) e.stopPropagation();
    };
    const onPointerDown = (e: PointerEvent) => {
      if (e.pointerType !== 'mouse' && !route(e)) e.stopPropagation();
    };
    el.addEventListener('contextmenu', onContextMenu, { capture: true });
    el.addEventListener('pointerdown', onPointerDown, { capture: true });
    onCleanup(() => {
      el.removeEventListener('contextmenu', onContextMenu, { capture: true });
      el.removeEventListener('pointerdown', onPointerDown, { capture: true });
    });
  };

  const menuItems = () => (menuTarget()?.kind === 'session' ? SESSION_ITEMS : GROUP_ITEMS);

  const onMenuSelect = (value: string) => {
    const t = menuTarget();
    if (!t) return;
    if (t.kind === 'session') {
      if (value === 'open-session') props.onSelectSession(t.session.session_id);
      else if (value === 'archive-session') props.onArchiveSession(t.session.session_id);
      else if (value === 'delete-session') props.onDeleteSession(t.session.session_id);
      return;
    }
    if (value === 'new-session') props.onNewSession(t.group.projectPath);
    else if (value === 'pin-project') props.onSelectProject(t.group.projectPath);
  };

  /**
   * One project header — chevron, name, count, and its New Session.
   *
   * Shared by the pinned list and the folded "Other projects" section, so a
   * folded project is the same row as a pinned one and starting work in it
   * is the same gesture.
   *
   * The chevron and the count only when there is a row to unfold. A project
   * whose every session sits in the Inbox above keeps its header, because
   * New Session belongs there, but a ">" over nothing promises a fold that
   * opens on air.
   */
  const groupHeader = (g: SessionGroup) => (
    <div
      // `bg-shell-bg` is the PANEL's own colour, so this reads as no fill at
      // all — it was `bg-shell-panel`, one step lighter, which banded every
      // project row against the list for no reason. The chevron and the
      // indent already say "this is a project"; a fill on top of that is
      // decoration. Sticky still needs SOME opaque paint, or rows
      // scroll through the header, so it paints the background it sits on.
      class="group/proj sticky top-0 z-10 flex items-center h-(--cru-row-sm) pr-1 bg-shell-bg hover:bg-hover-wash transition-colors"
      data-group-key={g.key}
    >
              <button
                type="button"
                class={`${treeGroupRow} gap-2 flex-1 min-w-0 h-full text-shell-ink hover:bg-transparent`}
                aria-expanded={g.rows.length ? !isCollapsed(g) : undefined}
                data-testid={`session-group-${g.key}`}
                onClick={() => g.rows.length && toggle(g.key)}
              >
                <Show
                  when={g.rows.length}
                  fallback={<span class={`${treeChevron} inline-block`} aria-hidden="true" />}
                >
                  <ChevronRight
                    data-testid="session-group-chevron"
                    class={`${treeChevron} ${isCollapsed(g) ? '' : 'rotate-90'}`}
                  />
                </Show>
                <span
                  classList={{
                    truncate: true,
                    'text-primary': props.currentProjectPath === g.projectPath && !!g.projectPath,
                  }}
                >
                  {g.name}
                </span>
                <Show when={g.rows.length}>
                  <span class="text-muted-dark font-normal tabular-nums">{g.rows.length}</span>
                </Show>
              </button>
              {/* Per-project New Session. On the PROJECT row because a session
                  belongs to exactly one project, and the panel-wide button it
                  replaces could not say which. Hidden on the project-less
                  group: there is no project there to start one in. */}
              <Show when={g.projectPath}>
                <button
                  type="button"
                  data-testid={`session-group-new-${g.key}`}
                  title={`New session in ${g.name}`}
                  aria-label={`New session in ${g.name}`}
                  onClick={() => props.onNewSession(g.projectPath)}
                  class="shrink-0 p-1 rounded text-muted-dark opacity-0 group-hover/proj:opacity-100 focus:opacity-100 [@media(hover:none)]:opacity-100 hover:text-shell-ink hover:bg-hover-wash transition-opacity"
                >
                  <Plus class="w-3 h-3" />
                </button>
      </Show>
    </div>
  );

  return (
    <div ref={attachContextRouter} class="contents">
    <Menu.Root onSelect={(d) => onMenuSelect(d.value)}>
    <Menu.ContextTrigger
      asChild={(triggerProps) => (
        <div {...triggerProps({ class: 'contents' })}>
    <div data-testid="session-tree" class="flex flex-col gap-2">
      {/* Above the project tier, because it is where you were. The waiting
          count rides its header in the accent. */}
      <TreeSection
        label="Inbox"
        count={inbox().length}
        open={inboxOpen()}
        onToggle={() => setInboxOpen((v) => !v)}
        testid="inbox-section"
        urgent={waitingCount() > 0}
      >
        <div class="flex flex-col">
          <For each={inbox()}>
            {(s) => (
              <SessionRow
                session={s}
                selected={props.currentSessionId === s.session_id}
                branch={branchOfSession(s)}
                kilnLabel={kilnNameOf(s)}
                projectLabel={projectLabelOf(s)}
                onSelect={() => props.onSelectSession(s.session_id)}
                onArchive={() => props.onArchiveSession(s.session_id)}
                onDelete={() => props.onDeleteSession(s.session_id)}
              />
            )}
          </For>
        </div>
      </TreeSection>
      <TreeSection
        label="Projects"
        count={groups().length}
        always
        open={projectsOpen()}
        onToggle={() => setProjectsOpen((v) => !v)}
        testid="projects-section"
        actions={props.projectsActions}
      >
        <For each={groups()}>
          {(g) => (
            <div>
              {/* Sticky: titles repeat across projects, so scrolling past a
                  header otherwise leaves nothing on screen saying which
                  project you are reading. */}
              {groupHeader(g)}
              <Show when={!isCollapsed(g) && g.rows.length > 0}>
                <div class="flex flex-col">
                  <For each={g.rows}>
                    {(s) => (
                      <SessionRow
                        session={s}
                        selected={props.currentSessionId === s.session_id}
                        branch={branchOfSession(s)}
                        kilnLabel={kilnNameOf(s)}
                        showKiln={kilnNameOf(s) !== dominantKiln(g)}
                        onSelect={() => props.onSelectSession(s.session_id)}
                        onArchive={() => props.onArchiveSession(s.session_id)}
                        onDelete={() => props.onDeleteSession(s.session_id)}
                      />
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </For>
      </TreeSection>

      {/* The projects the pin scopes out. Collapsed by default and counted,
          so the rail states what it hides without spending a row on each
          member. Empty when nothing is pinned: every project is above. */}
      <TreeSection
        label="Other projects"
        count={offScope().length}
        open={foldOpen()}
        onToggle={() => setIdleOpen((v) => !v)}
        testid="idle-projects-toggle"
      >
        <For each={offScope()}>
          {(g) => (
            <div>
              {groupHeader(g)}
              <Show when={!isCollapsed(g) && g.rows.length > 0}>
                <div class="flex flex-col">
                  <For each={g.rows}>
                    {(sn) => (
                      <SessionRow
                        session={sn}
                        selected={props.currentSessionId === sn.session_id}
                        branch={branchOfSession(sn)}
                        kilnLabel={kilnNameOf(sn)}
                        showKiln={kilnNameOf(sn) !== dominantKiln(g)}
                        onSelect={() => props.onSelectSession(sn.session_id)}
                        onArchive={() => props.onArchiveSession(sn.session_id)}
                        onDelete={() => props.onDeleteSession(sn.session_id)}
                      />
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </For>
      </TreeSection>
    </div>
        </div>
      )}
    />
      <Portal>
        <Menu.Positioner>
          <Menu.Content class={menuContent}>
            <For each={menuItems()}>
              {(item) => (
                <Menu.Item
                  value={item.value}
                  data-testid={`session-group-menu-${item.value}`}
                  class={menuItem}
                  classList={{ 'text-error': item.danger === true }}
                >
                  <item.icon class="w-3.5 h-3.5 shrink-0" />
                  <span>{item.label}</span>
                </Menu.Item>
              )}
            </For>
          </Menu.Content>
        </Menu.Positioner>
      </Portal>
    </Menu.Root>
    </div>
  );
};
