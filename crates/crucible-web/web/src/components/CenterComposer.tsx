import { Component, Show, createEffect, createSignal, on, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getConfig,
  getProviderTargets,
  getTargetProviders,
  isGitRepoUrl,
  listAgents,
  listAllModels,
  listKilns,
  listProjects,
  listProviders,
  scmClone,
  type ProviderTarget,
  type TargetProvider,
} from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { closeDraftTab } from '@/lib/draft-session';
import type {
  AgentProfileEntry,
  CreateSessionParams,
  KilnListEntry,
  Project,
} from '@/lib/types';
import { WorkingDots } from '@/components/AssistantTurn';
import { ComposerCard } from '@/components/composer/ComposerCard';
import { pathBasename } from '@/stores/statusBarStore';
import { syncRecentsFromServer } from '@/lib/recent-files';
import { kilnLabel } from '@/lib/kiln-label';
import { swrLocal } from '@/lib/local-cache';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
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

/**
 * The runtime chip's built-in "run here" row.
 *
 * Not a `provider:target` spec and deliberately not published by anything:
 * running on this machine is what happens when no provider is asked, so it
 * cannot depend on a plugin being installed. Picking it says "not isolated"
 * out loud, which the daemon acts on differently from saying nothing.
 */
const HOST = 'host';

/** A provider's mark, by the axis-agnostic name it published itself under. */
const PROVIDER_ICONS: Record<string, Component<{ class?: string }>> = {
  oci: Shield,
  ssh: Network,
  worktree: GitBranch,
  cloud: Cloud,
};
const iconForProvider = (plugin: string) => PROVIDER_ICONS[plugin] ?? FlaskConical;

/**
 * The session-creation surface — the content of a "New Session" tab. Context
 * chips (kiln / project / agent) over a prompt box with the model picker in
 * its footer. Nothing touches the daemon until the first message is sent
 * (lazy creation); the created chat docks right per WS-220 and this tab
 * closes behind it, leaving the center as the editing surface.
 *
 * This used to double as the empty-pane splash. An empty pane now renders
 * nothing — starting a session is a deliberate act (the ribbon's New Session,
 * the command palette), not something a pane falls back into.
 */
export const CenterComposer: Component<{ draftTabId?: string }> = (props) => {
  const { createSession } = useSessionSafe();

  const [agents, setAgents] = createSignal<AgentProfileEntry[]>([]);
  const [models, setModels] = createSignal<string[]>([]);
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [defaultKiln, setDefaultKiln] = createSignal('');
  const [defaultModel, setDefaultModel] = createSignal('');
  const [remoteShell, setRemoteShell] = createSignal(false);

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
  const [wsProviders, setWsProviders] = createSignal<TargetProvider[]>([]);
  const [rtProviders, setRtProviders] = createSignal<TargetProvider[]>([]);
  const [wsTargets, setWsTargets] = createSignal<Record<string, ProviderTarget[]>>({});
  const [rtTargets, setRtTargets] = createSignal<Record<string, ProviderTarget[]>>({});

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

  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [cloning, setCloning] = createSignal(false);

  const isAcp = () => agentName() !== '';

  onMount(() => {
    // No barrier, no blank chips: every source paints its LAST-KNOWN value
    // synchronously (swrLocal) and the fetch corrects it — a hard reload
    // shows real labels immediately instead of "Loading…"/fallback text.
    swrLocal('config', getConfig, (cfg) => {
      if (cfg?.kiln_path) setDefaultKiln(cfg.kiln_path);
      setRemoteShell(cfg?.remote_shell === true);
    });
    swrLocal('targets-workspace', () => getTargetProviders('workspace'), (p) => {
      if (p) setWsProviders(p);
    });
    swrLocal('targets-runtime', () => getTargetProviders('runtime'), (p) => {
      if (p) setRtProviders(p);
    });
    swrLocal('agents', listAgents, setAgents);
    swrLocal('models', () => listAllModels(), (mo) =>
      setModels(mo.filter((m) => !m.startsWith('[error]'))),
    );
    swrLocal('kilns', listKilns, setKilns);
    swrLocal('projects', () => listProjects(), setProjects);
    swrLocal('providers', () => listProviders(), (providers) => {
      const first = providers.find((p) => p.available);
      if (first?.default_model) setDefaultModel(first.default_model);
    });
    syncRecentsFromServer();
  });

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
  const runtimeParam = (): Partial<Pick<CreateSessionParams, 'isolation'>> => {
    const spec = runtime();
    if (!spec) return {};
    if (spec === HOST) return { isolation: false };
    const [plugin, ...rest] = spec.split(':');
    return { isolation: { plugin, target: rest.join(':') } };
  };

  const submit = async () => {
    const text = message().trim();
    if (!text || busy()) return;
    setBusy(true);
    try {
      await createSession(
        {
          // 'none' = an explicitly kiln-less session (v0.12 kiln-less
          // creation) — distinct from '' which falls back to the default.
          kilns: kiln() === 'none' ? [] : [kiln() || defaultKiln()].filter(Boolean),
          workspace: workspace() || undefined,
          ...(isAcp() ? { agent_type: 'acp', agent_name: agentName() } : {}),
          // The workspace axis: the daemon resolves this to a path before it
          // creates anything, so the session is born in the right checkout.
          ...(wsTarget() ? { workspace_target: wsTarget() } : {}),
          // The runtime axis. Spread on truthiness of the SPEC, then translated
          // — because `false` is an instruction the server acts on and must not
          // be dropped, while an untouched chip must send nothing at all.
          ...runtimeParam(),
        },
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

  // The default kiln's REGISTERED name (kiln.toml), not its path basename.
  const defaultKilnName = () => {
    const match = kilns().find((k) => k.path === defaultKiln());
    return kilnLabel(defaultKiln(), match?.name);
  };

  const kilnOptions = (): ChipOption[] => [
    { value: '', label: defaultKilnName(), hint: 'default' },
    ...kilns()
      .filter((k) => k.path !== defaultKiln())
      .map((k) => ({
        value: k.path,
        label: kilnLabel(k.path, k.name),
        hint: k.path,
      })),
    // Explicitly kiln-less — a session with no knowledge base attached.
    { value: 'none', label: 'No kiln' },
  ];

  /**
   * Re-enumerate every provider on one axis for the selected project.
   *
   * Providers answer per-project — a branch list belongs to a repo — so this
   * re-runs whenever the project chip changes. The guard is the reason it is
   * written out rather than dropped into a resource: a slow answer for project
   * A resolving after the user picked B must not land A's branches, or the
   * session is created against a worktree of the wrong repository.
   */
  const loadTargets = (
    providers: TargetProvider[],
    ws: string,
    set: (v: Record<string, ProviderTarget[]>) => void,
  ) => {
    if (providers.length === 0) return set({});
    void Promise.all(
      providers.map((p) => getProviderTargets(p, ws || undefined).then((t) => [p.plugin, t] as const)),
    ).then((entries) => {
      if (workspace() !== ws) return;
      set(Object.fromEntries(entries));
    });
  };

  createEffect(
    on([workspace, wsProviders, rtProviders], ([ws, wsp, rtp]) => {
      // A target chosen for the previous project names a branch that may not
      // exist in this one. Clearing is the only safe answer; keeping it would
      // silently resolve against a repo the user did not pick it in.
      setWsTarget('');
      loadTargets(wsp, ws, setWsTargets);
      loadTargets(rtp, ws, setRtTargets);
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
        const res = await scmClone(url);
        setProjects(await listProjects().catch(() => projects()));
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
      // folder ([scm] session_workspace_dir, default ~/.crucible/workspaces).
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
        <div class="w-full max-w-2xl flex flex-col gap-2">
          {/* Context chips — session scope reads as a sentence above the box. */}
          <div class="flex items-center justify-center gap-1 flex-wrap" data-testid="composer-context">
            <ChipSelect
              name="kiln"
              icon={FlaskConical}
              options={kilnOptions()}
              value={kiln()}
              onSelect={setKiln}
              disabled={busy()}
              testid="composer-kiln"
            />
            <ChipSelect
              name="project"
              icon={FolderGit2}
              options={
                cloning()
                  ? [{ value: workspace(), label: 'Cloning…', disabled: true }]
                  : projectOptions()
              }
              value={workspace()}
              onSelect={setWorkspace}
              disabled={busy() || cloning()}
              placeholder={cloning() ? 'Cloning…' : undefined}
              testid="composer-project"
              searchThreshold={1}
              create={{
                when: isGitRepoUrl,
                label: (url) => `Clone ${url} as new project`,
                run: cloneAndSelect,
              }}
              action={{
                label: 'Clone a repository…',
                placeholder: 'github.com/owner/repo or git URL',
                buttonLabel: 'Clone',
                validate: isGitRepoUrl,
                run: cloneAndSelect,
              }}
            />
            {/* The WORKSPACE axis — where the session's files live. Shown only
                when a provider actually offers something for this project, so
                a repo-less folder gets no chip rather than an empty one. This
                replaces the old branch chip, which called `scm.worktree_add`
                from here and confirmed with `window.confirm`; both are now the
                worktree plugin's business and neither is in the frontend. */}
            <Show when={wsOptions().length > 0}>
              <ChipSelect
                name="workspace"
                icon={GitBranch}
                options={wsOptions()}
                value={wsTarget()}
                onSelect={setWsTarget}
                disabled={busy()}
                placeholder={specLabel(wsOptions(), wsTarget()) ?? 'Workspace'}
                testid="composer-workspace-target"
                optionTestidPrefix="workspace-target"
              />
            </Show>
            {/* The RUNTIME axis — where the process runs (Cursor's "Run on"
                menu). One chip where there used to be two: a hardcoded target
                picker whose only enabled row was "This machine", and a separate
                isolation toggle. They were always the same question, and the
                answer is now whatever providers published. */}
            <ChipSelect
              name="run on"
              icon={Monitor}
              options={runtimeOptions()}
              value={runtime()}
              onSelect={setRuntime}
              disabled={busy()}
              placeholder={specLabel(runtimeOptions(), runtime()) ?? 'Run on'}
              testid="composer-target"
              optionTestidPrefix="runtime-target"
              footer={
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
                  <p class="mt-1 text-[11px] leading-snug text-muted-dark">
                    Reach this machine's sessions and terminal from other devices.
                    Configure via <code class="text-muted">[web] remote_shell</code> in
                    config.toml.
                  </p>
                </div>
              }
            />
            <ChipSelect
              name="agent"
              // The trigger wears the SELECTED agent's mark, so the chosen
              // agent is readable without opening the picker.
              icon={iconForAgent(agentName())}
              options={agentOptions()}
              value={agentName()}
              onSelect={setAgentName}
              disabled={busy()}
              testid="composer-agent"
            />
          </div>

          {/* The composer card — shared with the in-session chat input, so
              `/command` and `[[note]]` completion work here too. */}
          <ComposerCard
            value={message}
            setValue={setMessage}
            // The draft's selected kiln (or the daemon default once resolved)
            // backs `[[note]]` completion before the session exists.
            kilnPath={() => kiln() || defaultKiln()}
            placeholder="Plan, build, ask — a session starts with your first message"
            ariaLabel="First message"
            rows={3}
            testid="composer-input"
            onSubmit={() => void submit()}
            chips={
              <Show when={!isAcp()}>
                <ChipSelect
                  name="model"
                  options={modelOptions()}
                  value={model()}
                  onSelect={setModel}
                  disabled={busy()}
                  testid="composer-model"
                />
              </Show>
            }
            action={
              <button
                type="button"
                onClick={() => void submit()}
                disabled={!message().trim()}
                aria-label="Start session"
                title="Start session (Enter)"
                classList={{
                  'px-2.5 flex items-center justify-center transition-colors': true,
                  'bg-primary text-white hover:bg-primary-hover': !!message().trim(),
                  'bg-transparent text-muted-dark cursor-not-allowed': !message().trim(),
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
