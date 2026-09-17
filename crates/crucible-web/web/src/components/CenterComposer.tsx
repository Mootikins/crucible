import { Component, Show, createEffect, createSignal, on, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { isGitRepoUrl } from '@/lib/paths';
import type { ProviderTarget, TargetProvider } from '@/lib/types';
import { notificationActions } from '@/stores/notificationStore';
import { closeDraftTab } from '@/lib/draft-session';
import type { Project } from '@/lib/types';
import { WorkingDots } from '@/components/AssistantTurn';
import { ComposerCard } from '@/components/composer/ComposerCard';
import { pathBasename } from '@/stores/statusBarStore';
import { syncRecentsFromServer } from '@/lib/recent-files';
import { attachableKilns, kilnNameForPath, kilnPathForName } from '@/lib/kiln-registry';
import { HOST_RUNTIME, draftCreateParams, kilnsForCreate as kilnsToAttach } from '@/lib/session-draft';
import { useAgents } from '@/lib/query/agents';
import { useAllModels } from '@/lib/query/models';
import { useProviders } from '@/lib/query/providers';
import { useKilns } from '@/lib/query/kilns';
import { useConfig } from '@/lib/query/config';
import { useProjects, useScmClone } from '@/lib/query/projects';
import { useAxisTargets, useTargetProviders } from '@/lib/query/targets';
import type { ChipOption } from '@/components/composer/ChipSelect';
import type { ComposerChip } from '@/components/composer/ChipRow';
import { iconForAgent } from '@/lib/agent-icons';
import {
  ArrowUp,
  Bot,
  Cloud,
  FlaskConical,
  FolderGit2,
  GitBranch,
  Monitor,
  Network,
  Shield,
} from '@/lib/icons';

/** The runtime chip's built-in "run here" row — see `lib/session-draft.ts`. */
const HOST = HOST_RUNTIME;

/** A provider's mark, by the axis-agnostic name it published itself under. */
const PROVIDER_ICONS: Record<string, Component<{ class?: string }>> = {
  oci: Shield,
  ssh: Network,
  worktree: GitBranch,
  cloud: Cloud,
};
const iconForProvider = (plugin: string) => PROVIDER_ICONS[plugin] ?? FlaskConical;

/**
 * The session-creation surface — the content of a "New Session" tab. The
 * shared composer: a prompt capsule with the context chips (kiln / project /
 * workspace / runtime / agent / model) on the chip row under it, the same
 * row the live session draws. Nothing touches the daemon until the first message is sent
 * (lazy creation); the created chat docks right per WS-220 and this tab
 * closes behind it, leaving the center as the editing surface.
 *
 * This used to double as the empty-pane splash. An empty pane no longer falls
 * back into it — starting a session is a deliberate act (the ribbon's New
 * Session, the command palette). What an empty pane shows instead is
 * `windowing/EmptyPane`: the state it is in and the keys that fill it, and
 * nothing else.
 */
export const CenterComposer: Component<{
  draftTabId?: string;
  /** Project the draft opens aimed at (the sessions tree's per-project New
   * Session row). Seeds the project chip; the user may still change it. */
  workspace?: string;
}> = (props) => {
  const { createSession } = useSessionSafe();

  // The catalogue the phone's sheet reads too, under the `swrLocal('models')`
  // storage key it has always used. A name the daemon could not resolve comes
  // back as an `[error]` row, which is a diagnostic and not a model to offer.
  const modelsQuery = useAllModels();
  const models = () => (modelsQuery.data ?? []).filter((m) => !m.startsWith('[error]'));
  // The provider probe the session context runs, read through the same key:
  // the composer no longer pays for a second probe on mount.
  const providersQuery = useProviders();
  const defaultModel = () =>
    providersQuery.data?.find((p) => p.available)?.default_model ?? '';
  // The same roster the phone's sheet reads. It keeps the `swrLocal('agents')`
  // storage key, so the chip still paints its last-known names before the
  // daemon finishes probing each agent's binary.
  const agentsQuery = useAgents();
  const agents = () => agentsQuery.data ?? [];
  // One shared query, not this component's own fetch: the composer, the files
  // panel, the search panel and the scope chips all read the same roster.
  const kilnsQuery = useKilns();
  const kilns = () => kilnsQuery.data ?? [];
  // The same shared roster the project rail and the files pane read: a project
  // registered anywhere is in this chip's menu without a refetch of its own.
  const projectsQuery = useProjects();
  const projects = () => projectsQuery.data ?? [];
  const clone = useScmClone();
  // `config.kiln_path` — a PATH, and the only path left on this axis. It is
  // useful solely as a lookup key into the kiln list to recover the default's
  // registry NAME; nothing sends it anywhere.
  // The config chips paint their last-known value too: `useConfig` keeps the
  // `swrLocal('config')` storage key, and shares one request with the shell.
  const configQuery = useConfig();
  const defaultKilnPath = () => configQuery.data?.kiln_path ?? '';
  const remoteShell = () => configQuery.data?.remote_shell === true;

  // The two axes, both contributed by plugins. WORKSPACE answers where the
  // session's files live (a worktree, a checkout on another machine); RUNTIME
  // answers where its process runs (a container, an ssh host). They are
  // orthogonal and compose, which is why they are two chips and not one
  // setting — a session can run in a container against a worktree.
  //
  // Entirely opaque here: this component renders what providers published and
  // hands the pick back on create. It knows nothing about branches, images or
  // hosts, which is what lets a provider shipped tomorrow appear with no
  // change on this side. The branch chip it replaces did the opposite — it
  // called `scm.worktree_add` directly and put git in the rendering layer.
  const wsProvidersQuery = useTargetProviders('workspace');
  const rtProvidersQuery = useTargetProviders('runtime');
  const wsProviders = () => wsProvidersQuery.data ?? [];
  const rtProviders = () => rtProvidersQuery.data ?? [];

  // '' = internal agent / default kiln / default model / no project.
  const [agentName, setAgentName] = createSignal('');
  const [kiln, setKiln] = createSignal('');
  const [model, setModel] = createSignal('');
  const [workspace, setWorkspace] = createSignal('');
  // `provider:target` specs, or '' for "untouched". Empty is NOT the same as
  // an explicit pick: an untouched runtime lets the project's own setting
  // decide, while `host` overrides it. Collapsing the two would silently
  // containerize a session that opted out, or unsandbox one that did not.
  const [wsTarget, setWsTarget] = createSignal('');
  const [runtime, setRuntime] = createSignal('');

  // One cache entry per provider AND project, so the answer for the project
  // the user left cannot land on the chip of the project they picked. That is
  // what the hand-written out-of-order guard here used to do.
  const wsAxis = useAxisTargets('workspace', () => workspace() || undefined);
  const rtAxis = useAxisTargets('runtime', () => workspace() || undefined);
  const wsTargets = () => wsAxis.targets;
  const rtTargets = () => rtAxis.targets;

  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [cloning, setCloning] = createSignal(false);

  const isAcp = () => agentName() !== '';

  onMount(() => {
    syncRecentsFromServer();
  });

  // Follows the prop rather than seeding once: retargeting the open draft
  // (New Session on a second project) changes it, and a one-shot seed would
  // leave the chip naming the project the user just moved away from.
  createEffect(
    on(
      () => props.workspace,
      (ws) => {
        if (ws !== undefined) setWorkspace(ws);
      },
    ),
  );

  /**
   * The runtime chip's pick, as `session.create` takes it.
   *
   * Three outcomes, and they are three different instructions:
   *   untouched → nothing sent; the project's own setting decides
   *   This PC   → `false`; overrides the project, runs unisolated
   *   a target  → addressed to the provider that offered it
   *
   * Addressed rather than a bare name because more than one plugin answers on
   * this channel now, and a name meant for one used to be a hard error inside
   * another.
   */
  const submit = async () => {
    const text = message().trim();
    if (!text || busy()) return;
    setBusy(true);
    try {
      await createSession(
        // One copy of the empty-value contract, shared with the phone's sheet.
        draftCreateParams({
          kiln: kiln(),
          defaultKiln: defaultKilnName(),
          workspace: workspace(),
          agentName: agentName(),
          wsTarget: wsTarget(),
          runtime: runtime(),
        }),
        {
          initialMessage: text,
          model: !isAcp() && model() ? model() : undefined,
        },
      );
      setMessage('');
      // The real chat tab is open now — this draft has served its purpose.
      if (props.draftTabId) closeDraftTab(props.draftTabId);
    } catch {
      // Error surfaced via the session context's notification.
    } finally {
      // Always clear busy: on success closeDraftTab unmounts us, but a
      // draftTabId-less mount (or any failure) must re-enable the composer.
      setBusy(false);
    }
  };

  /**
   * The configured default kiln's REGISTRY name, or `null` when it has none.
   *
   * `null` is the ordinary case, not an error: `kiln.list` deliberately omits
   * the daemon data root, so a `kiln_path` still pointing at `~/.crucible`
   * matches no entry. There is no name to send for it and the daemon no longer
   * substitutes its data root, so "no name" and "no kiln" are the same
   * statement — the picker offers no default row and the session is born
   * kiln-less rather than born pointing at the session store.
   */
  const defaultKilnName = () => kilnNameForPath(defaultKilnPath(), kilns());

  /**
   * The rows this picker may offer.
   *
   * Registry names only, and only the ones the daemon says it can resolve: a
   * row it reports as `registered: false` names a directory `connect_kiln`
   * refuses. The rule lives in `attachableKilns` so this composer and the live
   * session's chips cannot drift apart on it.
   */
  const namedKilns = () => attachableKilns(kilns());

  const kilnOptions = (): ChipOption[] => [
    // '' = "whatever the config says", resolved to a name at submit. Offered
    // only when that name exists; otherwise there is nothing for it to mean.
    ...(defaultKilnName()
      ? [{ value: '', label: defaultKilnName() as string, hint: 'default' }]
      : []),
    ...namedKilns()
      .filter((k) => k.name !== defaultKilnName())
      .map((k) => ({
        value: k.name as string,
        label: k.name as string,
        // The path is the disambiguator, not the identity.
        hint: k.path,
      })),
    // Explicitly kiln-less — a session with no knowledge base attached.
    { value: 'none', label: 'No kiln' },
  ];

  /**
   * The `kilns` array for `session.create`, as registry NAMES.
   *
   * Three inputs collapse to two outcomes: 'none' and an unresolvable default
   * both mean the empty set, which the daemon now honours literally (§4.1,
   * tools-only session). Only a name that the registry answers for is ever
   * sent — a path here would name a directory the registration floor never
   * saw, and comes back 422.
   */
  const kilnsForCreate = (): string[] => kilnsToAttach(kiln(), defaultKilnName());

  /**
   * The selected kiln's directory, for the composer's wikilink autocomplete —
   * the one consumer on this screen that genuinely needs a path. Resolved
   * through the registry rather than reused from `config.kiln_path`, so an
   * unregistered directory completes against nothing instead of against a
   * corpus no session will actually be attached to.
   */
  const selectedKilnPath = () => kilnPathForName(kilnsForCreate()[0], kilns());

  createEffect(
    on(workspace, () => {
      // A target chosen for the previous project names a branch that may not
      // exist in this one. Clearing is the only safe answer; keeping it would
      // silently resolve against a repo the user did not pick it in.
      setWsTarget('');
    }),
  );

  /**
   * One axis's menu: a row per target, under a submenu per provider.
   *
   * Flattened when a single provider offers everything, because a submenu
   * containing the whole menu is not a submenu — it is an extra click. With
   * two or more, the drill-down earns its place and keeps the list short as
   * providers multiply.
   *
   * Values are `provider:target` specs, which is what the daemon splits on to
   * find who should answer.
   */
  const axisOptions = (
    providers: TargetProvider[],
    targets: Record<string, ProviderTarget[]>,
  ): ChipOption[] => {
    const rows = (p: TargetProvider): ChipOption[] =>
      (targets[p.plugin] ?? []).map((t) => ({
        value: t.spec,
        label: t.label,
        hint: t.hint,
        disabled: t.disabled,
      }));

    const offering = providers.filter((p) => rows(p).length > 0);
    if (offering.length === 1) return rows(offering[0]);
    return offering.map((p) => ({
      value: p.plugin,
      label: p.label,
      icon: iconForProvider(p.plugin),
      children: rows(p),
    }));
  };

  const wsOptions = () => axisOptions(wsProviders(), wsTargets());

  // "This PC" is built in, not published: running here is what happens when no
  // provider is asked, so no plugin has to exist for it to be an option — and
  // it is the only way to say "not isolated" out loud, which is a different
  // instruction from saying nothing.
  const runtimeOptions = (): ChipOption[] => [
    { value: HOST, label: 'This PC', icon: Monitor, hint: 'no isolation' },
    ...axisOptions(rtProviders(), rtTargets()),
  ];

  // Whether each axis's answer for the selected project has landed. Until it
  // has, the chip says only that the project's default applies; naming the
  // previous project's default in the meantime would name the wrong one.
  const wsReady = () => wsAxis.ready;
  const rtReady = () => rtAxis.ready;

  /** The provider row carrying `flag`, if any provider on the axis set one. */
  const flaggedTarget = (
    providers: TargetProvider[],
    targets: Record<string, ProviderTarget[]>,
    flag: 'default' | 'current',
  ): { provider: TargetProvider; target: ProviderTarget } | undefined => {
    for (const provider of providers) {
      const target = (targets[provider.plugin] ?? []).find((t) => t[flag]);
      if (target) return { provider, target };
    }
    return undefined;
  };

  /**
   * What an untouched runtime chip actually gets, named.
   *
   * Read off the providers, never derived here: the provider that would
   * claim the session when it says nothing flags that row `default` (the
   * oci plugin's unnamed row — its devcontainer or configured image, by its
   * own precedence). No provider flagging one means no provider claims the
   * session, and the daemon then runs it on this machine — the one rule the
   * client states, because it is structural rather than policy: with no claim
   * there is nothing else the process could run in.
   */
  const defaultRuntimeLabel = () => {
    if (!rtReady()) return 'Project default';
    const hit = flaggedTarget(rtProviders(), rtTargets(), 'default');
    if (!hit) return 'This PC · default';
    const name =
      hit.target.label === 'Default'
        ? hit.provider.label
        : `${hit.provider.label} · ${hit.target.label}`;
    return `${name} · default`;
  };

  /**
   * What an untouched workspace chip gets: the checkout the project already
   * is, which the worktree provider marks `current`. A project no provider
   * answers for has no name to show, so the generic placeholder stays.
   */
  const defaultWorkspaceLabel = () => {
    if (!wsReady()) return 'Project default';
    const hit = flaggedTarget(wsProviders(), wsTargets(), 'current');
    return hit ? `${hit.target.label} · default` : 'Project default';
  };

  /** The label for a chosen spec, looked up through any submenu. */
  const specLabel = (options: ChipOption[], spec: string): string | undefined => {
    for (const option of options) {
      if (option.value === spec && !option.children?.length) return option.label;
      const child = option.children?.find((c) => c.value === spec);
      if (child) return `${option.label} · ${child.label}`;
    }
    return undefined;
  };


  // Paste a git URL (or owner/repo) into the project popout's filter to
  // clone-and-select without a side-panel detour — the session starts
  // against the fresh checkout.
  const cloneAndSelect = (url: string) => {
    setCloning(true);
    void (async () => {
      try {
        // The mutation refreshes the roster before it settles, so the new
        // checkout is a row in the chip's menu by the time it is selected.
        const res = await clone.mutateAsync(url);
        setWorkspace(res.path);
        notificationActions.addNotification('info', `Cloned ${url} → ${res.path}`);
      } catch (err) {
        notificationActions.addNotification(
          'error',
          err instanceof Error ? err.message : 'Failed to clone repository',
        );
      } finally {
        setCloning(false);
      }
    })();
  };

  const projectOptions = (): ChipOption[] => {
    const main = projects().filter((p) => !p.repository?.is_worktree);
    const worktrees = projects().filter((p) => p.repository?.is_worktree);
    const wtLabel = (p: Project) => {
      const root = p.repository?.root;
      const rel = root && p.path.startsWith(root + '/') ? p.path.slice(root.length + 1) : null;
      const repo = root ? pathBasename(root) || root : null;
      return rel && repo ? `${repo} › ${rel}` : p.name || pathBasename(p.path) || p.path;
    };
    const label = (p: Project) =>
      p.repository?.is_worktree ? wtLabel(p) : p.name || pathBasename(p.path) || p.path;
    // Recents section (Cursor's picker leads with it): the daemon already
    // sorts projects by last_accessed. Only worth a section once the full
    // list is long enough that recency actually saves scanning.
    const recents =
      projects().length > 4
        ? projects().slice(0, 3).map((p) => ({
            value: p.path,
            label: label(p),
            hint: p.path,
            group: 'Recents',
          }))
        : [];
    return [
      ...recents,
      // No project selected → the daemon gives the session its own scratch
      // folder ([workspace] session_scratch_dir, default ~/.crucible/workspaces).
      { value: '', label: 'Session folder', hint: 'unique per session', group: 'Projects' },
      ...main.map((p) => ({
        value: p.path,
        label: p.name || pathBasename(p.path) || p.path,
        group: 'Projects',
      })),
      ...worktrees.map((p) => ({
        value: p.path,
        label: wtLabel(p),
        group: 'Worktrees',
      })),
    ];
  };

  const agentOptions = (): ChipOption[] => [
    { value: '', label: 'Internal agent', icon: Bot },
    ...agents().map((a) => ({
      value: a.name,
      label: a.name,
      hint: a.available ? a.description : 'not installed',
      disabled: !a.available,
      icon: iconForAgent(a.name),
    })),
  ];

  const modelOptions = (): ChipOption[] => [
    // '' = provider default. No placeholder on the chip, so an unset model
    // reads as the 'Auto' row that is actually selected rather than implying
    // a choice is still owed.
    { value: '', label: 'Auto', hint: defaultModel() || 'provider default' },
    ...models().map((m) => ({ value: m, label: m })),
  ];

  /** The runtime chip's footer: the remote-control state, read-only. Built
   * once; the chip list below is rebuilt on every signal it reads. */
  const remoteControlFooter = (
                <div class="m-1.5 mt-1 rounded-md border border-hairline bg-surface-base p-2.5">
                  <div class="flex items-center justify-between gap-2">
                    <span class="text-xs font-medium text-shell-ink">Remote control</span>
                    <span
                      classList={{
                        'relative inline-block w-7 h-4 rounded-full transition-colors': true,
                        'bg-primary/60': remoteShell(),
                        'bg-surface-elevated border border-hairline': !remoteShell(),
                      }}
                      role="img"
                      aria-label={remoteShell() ? 'Remote control on' : 'Remote control off'}
                      data-testid="remote-control-state"
                    >
                      <span
                        classList={{
                          'absolute top-0.5 w-3 h-3 rounded-full bg-shell-ink transition-all': true,
                          'left-3.5': remoteShell(),
                          'left-0.5 opacity-50': !remoteShell(),
                        }}
                      />
                    </span>
                  </div>
                  <p class="mt-1 text-floor leading-snug text-muted-dark">
                    Reach this machine's sessions and terminal from other devices.
                    Configure via <code class="text-muted">web.remote_shell</code> in your
                    init.lua.
                  </p>
                </div>
  );

  /**
   * The draft's chip row, as data for the shared `ChipRow`.
   *
   * Every entry is one axis of `session.create`, and '' on every one means
   * "untouched", shown as what that resolves to. The workspace chip appears
   * only when a provider offers something for this project, so a repo-less
   * folder gets no chip rather than an empty one. The WORKSPACE axis is
   * where the session's files live; the RUNTIME axis is where its process
   * runs (Cursor's "Run on" menu) — one chip where there used to be a
   * hardcoded target picker and a separate isolation toggle. Both render
   * what providers published and hand the pick back on create.
   */
  // The row marks a default itself (`<label> · default`), so a label that
  // already carries the word hands over the bare name. The generic "Project
  // default" (no provider has answered yet) is not a name and stays as it
  // is, unmarked.
  const bareDefault = (label: string) =>
    label === 'Project default' ? undefined : label.replace(/ · default$/, '');

  /**
   * The draft's chip row, as data.
   *
   * `priority` is the draw order AND the fold order: the row shows as many
   * whole chips as its width holds and folds the rest into its `+N` button,
   * from the right. So the two axes a user changes most (the model, the
   * kiln) lead, and the two the project already answers for (the workspace
   * target, the runtime) are the first to go. The list below is grouped by
   * subject rather than by priority, which is why each entry says its own.
   */
  const draftChips = (): ComposerChip[] => [
    {
      key: 'kiln',
      priority: 20,
      label: 'Kiln',
      value: kiln(),
      defaultLabel: defaultKilnName() ?? 'No kiln',
      icon: FlaskConical,
      options: kilnOptions(),
      onSelect: setKiln,
      disabled: busy(),
      testid: 'composer-kiln',
    },
    {
      key: 'project',
      priority: 30,
      label: 'Project',
      value: workspace(),
      defaultLabel: cloning() ? 'Cloning…' : 'Session folder',
      valueLabel: cloning() ? 'Cloning…' : undefined,
      icon: FolderGit2,
      options: cloning()
        ? [{ value: workspace(), label: 'Cloning…', disabled: true }]
        : projectOptions(),
      onSelect: setWorkspace,
      disabled: busy() || cloning(),
      testid: 'composer-project',
      select: {
        searchThreshold: 1,
        create: {
          when: isGitRepoUrl,
          label: (url) => `Clone ${url} as new project`,
          run: cloneAndSelect,
        },
        action: {
          label: 'Clone a repository…',
          placeholder: 'github.com/owner/repo or git URL',
          buttonLabel: 'Clone',
          validate: isGitRepoUrl,
          run: cloneAndSelect,
        },
      },
    },
    ...(wsOptions().length > 0
      ? [
          {
            key: 'workspaceTarget',
            priority: 50,
            label: 'Workspace',
            value: wsTarget(),
            // The actual default and not the axis name: the chip already
            // wears the role, so an unset value says what happens instead.
            defaultLabel: bareDefault(defaultWorkspaceLabel()),
            valueLabel: specLabel(wsOptions(), wsTarget()),
            select: {
              optionTestidPrefix: 'workspace-target',
              placeholder: bareDefault(defaultWorkspaceLabel()) ? undefined : defaultWorkspaceLabel(),
            },
            icon: GitBranch,
            options: wsOptions(),
            onSelect: setWsTarget,
            disabled: busy(),
            testid: 'composer-workspace-target',
          } satisfies ComposerChip,
        ]
      : []),
    {
      key: 'runtime',
      priority: 60,
      label: 'Run on',
      value: runtime(),
      defaultLabel: bareDefault(defaultRuntimeLabel()),
      valueLabel: specLabel(runtimeOptions(), runtime()),
      select: {
        optionTestidPrefix: 'runtime-target',
        footer: remoteControlFooter,
        placeholder: bareDefault(defaultRuntimeLabel()) ? undefined : defaultRuntimeLabel(),
      },
      icon: Monitor,
      options: runtimeOptions(),
      onSelect: setRuntime,
      disabled: busy(),
      testid: 'composer-target',
    },
    {
      key: 'agent',
      priority: 40,
      label: 'Agent',
      value: agentName(),
      defaultLabel: 'Internal agent',
      // The trigger wears the SELECTED agent's mark, so the chosen agent is
      // readable without opening the picker.
      icon: iconForAgent(agentName()),
      options: agentOptions(),
      onSelect: setAgentName,
      disabled: busy(),
      testid: 'composer-agent',
    },
    ...(isAcp()
      ? []
      : [
          {
            key: 'model',
            priority: 10,
            label: 'Model',
            value: model(),
            defaultLabel: defaultModel() || 'Auto',
            options: modelOptions(),
            onSelect: setModel,
            disabled: busy(),
            testid: 'composer-model',
          } satisfies ComposerChip,
        ]),
  ];

  return (
    <div class="flex-1 h-full bg-shell-bg flex flex-col items-center justify-center p-6 overflow-y-auto" data-testid="center-composer">
      <Show
        when={!busy()}
        fallback={
          <div class="w-full max-w-2xl flex flex-col gap-4" data-testid="composer-pending">
            <div class="user-quote">
              <p class="whitespace-pre-wrap break-words">{message().trim()}</p>
            </div>
            <WorkingDots />
          </div>
        }
      >
        <div class="w-full max-w-2xl">
          {/* The composer card — shared with the in-session chat input: the
              same capsule, the same chip row under it, and `/command` and
              `[[note]]` completion. Only the chip LIST differs. */}
          <ComposerCard
            value={message}
            setValue={setMessage}
            // The draft's selected kiln (or the config default once resolved)
            // backs `[[note]]` completion before the session exists.
            kilnPath={selectedKilnPath}
            placeholder="Plan, build, ask — a session starts with your first message"
            ariaLabel="First message"
            // One line at rest, so the draft is a pill until the message
            // needs a second line — the same shape the in-session prompt has.
            rows={1}
            testid="composer-input"
            onSubmit={() => void submit()}
            chips={draftChips()}
            action={
              <button
                type="button"
                onClick={() => void submit()}
                disabled={!message().trim()}
                aria-label="Start session"
                title="Start session (Enter)"
                classList={{
                  // Same geometry and same disabled treatment as the
                  // in-session send (ChatInput's SEND_BASE). The two are the
                  // same affordance on two surfaces and a user should not
                  // have to learn it twice.
                  'focus-ring flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-colors': true,
                  'bg-primary text-on-primary hover:bg-primary-hover': !!message().trim(),
                  'bg-control text-muted-dark cursor-not-allowed': !message().trim(),
                }}
                data-testid="composer-send"
              >
                <ArrowUp class="w-4 h-4" />
              </button>
            }
          />

        </div>
      </Show>
    </div>
  );
};
