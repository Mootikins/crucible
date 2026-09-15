import { Component, For, Show, createMemo, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { BottomSheet, SheetOption } from '@/components/mobile/BottomSheet';
import { SessionStatusDot } from '@/components/shell/SessionStatusDot';
import { sessionDisplayTitle } from '@/lib/session-display';
import { sessionWorkspace } from '@/lib/session-scope';
import { sessionStatus } from '@/lib/session-status';
import { byRecency, inboxSessions } from '@/lib/session-inbox';
import { reflectionSessions } from '@/lib/session-reflections';
import { terseAge } from '@/lib/format-time';
import { ChevronDown, GitBranch, Plus } from '@/lib/icons';
import { treeChevron, treeRow, treeSectionHeader } from '@/components/tree/tree-style';
import { sessionDefaultKiln } from '@/lib/session-scope';
import type { Session } from '@/lib/types';

/** The switcher's "every project" choice. Not a path, so it collides with none. */
const ALL_PROJECTS = '__all__';

/**
 * The phone's Sessions tab: one project at a time, with a switcher.
 *
 * The desktop rail nests every project's sessions in one tree. On a phone that
 * tree is too long to scan, so the header chooses a project and the list below
 * shows its sessions, most recent first. **The switcher is what lets a user
 * leave the recency list** — without it a phone can only reach what it touched
 * last.
 *
 * The Inbox ignores the switcher: the last few sessions the user touched
 * matter whatever project is on screen, so it stays cross-project, as on the
 * desktop. The list below leaves them to it, as the desktop tree does: one
 * session, one row.
 */
export const SessionsTab: Component = () => {
  const { sessions, currentSession, selectSession, refreshSessions } = useSessionSafe();
  const { projects, currentProject, selectProject } = useProjectSafe();
  const [picking, setPicking] = createSignal(false);
  const [scope, setScope] = createSignal<string | null>(null);

  onMount(() => refreshSessions({ includeArchived: false }));

  /** The project on screen: the user's pick, else the app's current project. */
  const chosen = () => scope() ?? currentProject()?.path ?? ALL_PROJECTS;
  const chosenName = () =>
    chosen() === ALL_PROJECTS
      ? 'All projects'
      : (projects().find((p) => p.path === chosen())?.name ?? chosen());

  const active = createMemo(() => sessions().filter((s) => !s.archived));
  const inbox = createMemo(() => inboxSessions(active()));
  const inboxIds = createMemo(() => new Set(inbox().map((s) => s.id)));
  /**
   * The passes a plugin ran for itself — see `lib/session-reflections.ts`.
   *
   * Cross-project like the Inbox, and for a stronger reason: a pass has no
   * workspace, so no project's list holds it and the switcher cannot reach
   * it. Its own section is the only way a phone meets one.
   */
  const reflections = createMemo(() => reflectionSessions(active()));
  /** Every session of the chosen project, Inbox members included. */
  const inScope = createMemo(() => {
    const target = chosen();
    const all = [...active()].filter((s) => s.session_type !== 'plugin').sort(byRecency);
    return target === ALL_PROJECTS ? all : all.filter((s) => sessionWorkspace(s) === target);
  });
  /** The rows the project list draws: `inScope` minus what the Inbox shows. */
  const listed = createMemo(() => inScope().filter((s) => !inboxIds().has(s.id)));

  const projectOf = (s: Session) => {
    const workspace = sessionWorkspace(s);
    return projects().find((p) => p.path === workspace)?.name ?? null;
  };

  // The desktop row's vocabulary — `SessionTree.tsx`'s `SessionRow` — at a
  // thumb's height. Same tints, same text sizes, same chips; only the row box
  // and the target size differ, because 26 px is not tappable.
  const Row = (props: { session: Session; showProject?: boolean }) => (
    <button
      type="button"
      class={`${treeRow} w-full h-11 px-3 flex items-center gap-2 rounded text-left transition-colors focus-ring ${
        currentSession()?.id === props.session.id
          ? 'bg-primary/10 text-shell-ink'
          : 'hover:bg-hover-wash text-shell-body'
      }`}
      data-session-id={props.session.id}
      onClick={() => void selectSession(props.session.id)}
    >
      <SessionStatusDot status={sessionStatus(props.session)} />
      {/* No text size here: the row takes it from the density attribute, so
          the phone gets the touch size and the file tree gets the same one. */}
      <span class="flex-1 min-w-0 truncate">{sessionDisplayTitle(props.session)}</span>
      <Show when={sessionDefaultKiln(props.session)} keyed>
        {(kilnName) => (
          <span class="shrink-0 truncate max-w-[80px] text-floor text-muted-dark">{kilnName}</span>
        )}
      </Show>
      <Show when={props.showProject && projectOf(props.session)}>
        {(name) => (
          <span
            class="shrink-0 inline-flex items-center gap-1 px-1 rounded bg-surface-elevated border border-hairline text-floor text-muted-dark"
            title={`project · ${name()}`}
          >
            <GitBranch class="w-2.5 h-2.5 shrink-0" />
            <span class="truncate max-w-[80px]">{name()}</span>
          </span>
        )}
      </Show>
      <span class="w-8 shrink-0 text-right text-floor text-muted-dark">
        {terseAge(props.session.last_activity ?? props.session.started_at) ?? ''}
      </span>
    </button>
  );

  return (
    <div class="flex-1 min-h-0 flex flex-col" data-density="touch">
      <div class="shrink-0 flex items-center gap-1 px-2 py-1 border-b border-hairline">
        <button
          type="button"
          aria-label={`Project: ${chosenName()}`}
          class="flex-1 h-11 px-2 flex items-center gap-1 rounded text-left text-xs font-medium text-muted hover:bg-hover-wash transition-colors focus-ring"
          onClick={() => setPicking(true)}
        >
          <span class="flex-1 truncate">{chosenName()}</span>
          <ChevronDown class={`${treeChevron} text-muted-dark`} />
        </button>
        <Show when={chosen() !== ALL_PROJECTS}>
          <button
            type="button"
            aria-label={`New session in ${chosenName()}`}
            class="w-11 h-11 flex items-center justify-center shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash focus-ring"
            onClick={() =>
              window.dispatchEvent(
                new CustomEvent('crucible:new-session', { detail: { workspace: chosen() } }),
              )
            }
          >
            <Plus class="w-5 h-5" />
          </button>
        </Show>
      </div>

      <div class="flex-1 min-h-0 overflow-y-auto p-1">
        <Show when={inbox().length > 0}>
          <section data-testid="compact-inbox" class="mb-2">
            <h2 class={treeSectionHeader}>Inbox ({inbox().length})</h2>
            {/* A gap between rows: two tinted rows that touch read as one
                block, and the tint is what says which session is open. */}
            <div class="flex flex-col gap-0.5 px-1">
              <For each={inbox()}>{(s) => <Row session={s} showProject />}</For>
            </div>
          </section>
        </Show>

        <Show when={reflections().length > 0}>
          <section data-testid="compact-reflections" class="mb-2">
            <h2 class={treeSectionHeader}>Reflections ({reflections().length})</h2>
            <div class="flex flex-col gap-0.5 px-1">
              <For each={reflections()}>{(s) => <Row session={s} />}</For>
            </div>
          </section>
        </Show>

        <section>
          <h2 class={treeSectionHeader}>{chosenName()}</h2>
          <div class="flex flex-col gap-0.5 px-1">
            <For each={listed()}>
              {(s) => <Row session={s} showProject={chosen() === ALL_PROJECTS} />}
            </For>
          </div>
          {/* Only when the project has nothing at all. A project whose every
              session sits in the Inbox is not empty; its rows are above. */}
          <Show when={inScope().length === 0}>
            <p class="px-3 py-6 text-center text-reading text-muted-dark">No sessions here yet.</p>
          </Show>
        </section>
      </div>

      <BottomSheet open={picking()} label="Choose a project" onClose={() => setPicking(false)}>
        <SheetOption
          label="All projects"
          selected={chosen() === ALL_PROJECTS}
          onSelect={() => {
            setScope(ALL_PROJECTS);
            setPicking(false);
          }}
        />
        <For each={projects()}>
          {(project) => (
            <SheetOption
              label={project.name || project.path}
              selected={chosen() === project.path}
              onSelect={() => {
                setScope(project.path);
                void selectProject(project.path);
                setPicking(false);
              }}
            />
          )}
        </For>
      </BottomSheet>
    </div>
  );
};
