import { Component, Show, createEffect, createSignal, on, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getConfig,
  getIsolationOffer,
  isBranchNameish,
  isGitRepoUrl,
  listAgents,
  listAllModels,
  listKilns,
  listProjects,
  listProviders,
  registerProject,
  scmBranches,
  scmClone,
  scmWorktreeAdd,
  type IsolationOffer,
  type ScmBranchesResponse,
} from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { closeDraftTab } from '@/lib/draft-session';
import type { AgentProfileEntry, KilnListEntry, Project } from '@/lib/types';
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
  // What the box offers for isolating a session, published by whichever
  // plugin(s) provide it. Opaque to this component: it renders the offer and
  // hands the pick back on create, and never reads plugin config to infer it.
  const [isolationOffer, setIsolationOffer] = createSignal<IsolationOffer>({
    available: false,
    profiles: [],
  });
  const profiles = () => isolationOffer().profiles;

  // '' = internal agent / default kiln / default model / no project.
  const [agentName, setAgentName] = createSignal('');
  const [kiln, setKiln] = createSignal('');
  const [model, setModel] = createSignal('');
  const [workspace, setWorkspace] = createSignal('');
  // undefined = untouched (the server resolves normally), false = explicitly
  // no isolation, true = isolate with the server's default, string = a named
  // profile. `undefined` and `false` are DIFFERENT instructions, so this is a
  // three-state control, not a boolean.
  const [isolation, setIsolation] = createSignal<string | boolean | undefined>(undefined);

  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [cloning, setCloning] = createSignal(false);
  // Branch data for the selected project's repo (Cursor's `repo ˅ branch ˅`
  // pair): fetched whenever the workspace chip changes to a git checkout.
  const [repoBranches, setRepoBranches] = createSignal<ScmBranchesResponse | null>(null);
  const [branchBusy, setBranchBusy] = createSignal(false);

  const isAcp = () => agentName() !== '';

  onMount(() => {
    // No barrier, no blank chips: every source paints its LAST-KNOWN value
    // synchronously (swrLocal) and the fetch corrects it — a hard reload
    // shows real labels immediately instead of "Loading…"/fallback text.
    swrLocal('config', getConfig, (cfg) => {
      if (cfg?.kiln_path) setDefaultKiln(cfg.kiln_path);
      setRemoteShell(cfg?.remote_shell === true);
    });
    swrLocal('isolation-offer', getIsolationOffer, (offer) => {
      if (offer) setIsolationOffer(offer);
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

  const submit = async () => {
    const text = message().trim();
    if (!text || busy()) return;
    setBusy(true);
    try {
      await createSession(
        {
          // 'none' = an explicitly kiln-less session (v0.12 kiln-less
          // creation) — distinct from '' which falls back to the default.
          kiln: kiln() === 'none' ? undefined : kiln() || defaultKiln() || undefined,
          workspace: workspace() || undefined,
          ...(isAcp() ? { agent_type: 'acp', agent_name: agentName() } : {}),
          // Spread on `!== undefined`, never on truthiness: `false` is an
          // instruction the server acts on, not a value to drop.
          ...(isolation() !== undefined ? { isolation: isolation()! } : {}),
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

  createEffect(
    on(workspace, (ws) => {
      setRepoBranches(null);
      if (!ws) return;
      // Out-of-order guard: a slow scmBranches(A) resolving after the user
      // switched to project B must not land A's branches (a branch pick
      // would then create worktrees on the WRONG repo).
      scmBranches(ws)
        .then((res) => workspace() === ws && setRepoBranches(res))
        .catch(() => workspace() === ws && setRepoBranches(null)); // repo-less project: no chip
    }),
  );

  const currentBranch = () => repoBranches()?.branches.find((b) => b.is_current)?.name ?? '';

  const branchOptions = (): ChipOption[] =>
    (repoBranches()?.branches ?? []).map((b) => ({
      value: b.name,
      label: b.name,
      hint: b.is_current
        ? 'current'
        : b.worktree_path
          ? pathBasename(b.worktree_path) || undefined
          : b.remote_only
            ? 'remote · new worktree'
            : 'new worktree',
    }));

  // Selecting a branch moves the session's workspace to that branch's
  // checkout — jumping to an existing worktree, or creating one (the
  // parallel-agents flow: N sessions × N worktrees without leaving the
  // composer).
  const switchToCheckout = async (path: string) => {
    try {
      await registerProject(path);
    } catch {
      /* already registered */
    }
    setProjects(await listProjects().catch(() => projects()));
    setWorkspace(path);
  };

  const pickBranch = (name: string) => {
    const repo = repoBranches();
    const branch = repo?.branches.find((b) => b.name === name);
    if (!repo || !branch || branch.is_current) return;
    setBranchBusy(true);
    void (async () => {
      try {
        if (branch.worktree_path) {
          await switchToCheckout(branch.worktree_path);
        } else {
          if (!window.confirm(`No worktree for '${name}' — create one?`)) return;
          const res = await scmWorktreeAdd(repo.repo_root, name, false);
          if (res.warning) notificationActions.addNotification('warning', res.warning);
          setProjects(await listProjects().catch(() => projects()));
          setWorkspace(res.path);
        }
      } catch (err) {
        notificationActions.addNotification(
          'error',
          err instanceof Error ? err.message : 'Failed to switch branch',
        );
      } finally {
        setBranchBusy(false);
      }
    })();
  };

  const createBranchWorktree = (name: string) => {
    const repo = repoBranches();
    if (!repo) return;
    if (!window.confirm(`Create branch '${name}' and a worktree for it?`)) return;
    setBranchBusy(true);
    void (async () => {
      try {
        const res = await scmWorktreeAdd(repo.repo_root, name, true);
        if (res.warning) notificationActions.addNotification('warning', res.warning);
        setProjects(await listProjects().catch(() => projects()));
        setWorkspace(res.path);
      } catch (err) {
        notificationActions.addNotification(
          'error',
          err instanceof Error ? err.message : 'Failed to create branch',
        );
      } finally {
        setBranchBusy(false);
      }
    })();
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

  // ---- isolation (toggle + profile select) --------------------------------
  //
  // Generic by construction: `profiles` is a list of names from /api/config
  // and the chosen name goes back on create untouched. Nothing here knows what
  // a profile selects, or what kind of thing does the isolating — that is the
  // resolving plugin's business, and keeping it there is what lets a plugin
  // shipped tomorrow reuse this control unchanged.
  const isolationOn = () => isolation() !== undefined && isolation() !== false;
  const isolationProfile = () => (typeof isolation() === 'string' ? (isolation() as string) : '');
  const toggleIsolation = () => setIsolation(isolationOn() ? false : true);

  const isolationLabel = () => {
    const value = isolation();
    if (value === undefined) return 'Isolation';
    if (value === false) return 'No isolation';
    return value === true ? 'Isolated' : `Isolated · ${value}`;
  };

  const isolationOptions = (): ChipOption[] => [
    { value: '', label: 'Default profile', hint: 'server default', group: 'Profile' },
    ...profiles().map((p) => ({ value: p, label: p, group: 'Profile' })),
  ];

  // A profile pick implies "on" — nobody names a profile to then not use it.
  const pickProfile = (name: string) => setIsolation(name || true);

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
            {/* Branch chip — only once the selected project is a git repo
                (Cursor's `repo ˅ branch ˅` pair). Picking a branch moves the
                workspace to that branch's worktree, creating it on demand. */}
            <Show when={repoBranches()}>
              <ChipSelect
                name="branch"
                icon={GitBranch}
                options={branchOptions()}
                value={currentBranch()}
                onSelect={pickBranch}
                disabled={busy() || branchBusy()}
                placeholder={branchBusy() ? 'Switching…' : 'branch'}
                testid="composer-branch"
                create={{
                  when: isBranchNameish,
                  label: (name) => `Create branch + worktree '${name}'`,
                  run: createBranchWorktree,
                }}
              />
            </Show>
            {/* Execution target (Cursor's "Run on" menu). Only the local
                machine exists today — cloud/remote rows are the declared
                seam for the containers/targets design. Worktree creation
                deliberately lives in the branch/project menus, not here. */}
            <ChipSelect
              name="run on"
              icon={Monitor}
              options={[
                { value: 'local', label: 'This machine', group: 'Run on', icon: Monitor },
                {
                  value: 'cloud',
                  label: 'Cloud',
                  group: 'Run on',
                  icon: Cloud,
                  disabled: true,
                  hint: 'planned',
                },
                {
                  value: 'remote',
                  label: 'Remote machines',
                  group: 'Run on',
                  icon: Network,
                  disabled: true,
                  hint: 'planned',
                },
              ]}
              value="local"
              onSelect={() => {}}
              disabled={busy()}
              testid="composer-target"
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
                    Configure via <code class="text-muted">[server] remote_shell</code> in
                    config.toml.
                  </p>
                </div>
              }
            />
            {/* Isolation — a toggle plus whatever profile names were
                published. Gated on the box OFFERING isolation, not on there
                being named profiles: the documented config is a bare image
                with no profiles table, and gating on names hid the control
                exactly there — leaving no way to opt a session *out*. A
                control that can only fail is still worse than none, so a box
                offering nothing shows nothing. */}
            <Show when={isolationOffer().available}>
              <ChipSelect
                name="isolation"
                icon={Shield}
                options={isolationOptions()}
                value={isolationProfile()}
                triggerLabel={isolationLabel()}
                onSelect={pickProfile}
                disabled={busy()}
                testid="composer-isolation"
                footer={
                  <div class="m-1.5 mt-1 rounded-md border border-hairline bg-surface-base p-2.5">
                    <button
                      type="button"
                      onClick={toggleIsolation}
                      aria-pressed={isolationOn()}
                      class="w-full flex items-center justify-between gap-2"
                      data-testid="isolation-toggle"
                    >
                      <span class="text-xs font-medium text-shell-ink">Isolate this session</span>
                      <span
                        classList={{
                          'relative inline-block w-7 h-4 rounded-full transition-colors': true,
                          'bg-primary/60': isolationOn(),
                          'bg-surface-elevated border border-hairline': !isolationOn(),
                        }}
                      >
                        <span
                          classList={{
                            'absolute top-0.5 w-3 h-3 rounded-full bg-shell-ink transition-all': true,
                            'left-3.5': isolationOn(),
                            'left-0.5 opacity-50': !isolationOn(),
                          }}
                        />
                      </span>
                    </button>
                    <p class="mt-1 text-[11px] leading-snug text-muted-dark">
                      {isolation() === undefined
                        ? 'Untouched — this session follows the project’s own setting.'
                        : isolationOn()
                          ? 'This session runs in the selected environment.'
                          : 'This session runs unisolated, even if the project asks otherwise.'}
                    </p>
                  </div>
                }
              />
            </Show>
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
