import { Component, For, Show, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getConfig,
  isGitRepoUrl,
  listAgents,
  listAllModels,
  listKilns,
  listProjects,
  listProviders,
  scmClone,
} from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import type { AgentProfileEntry, KilnListEntry, Project } from '@/lib/types';
import { WorkingDots } from '@/components/AssistantTurn';
import { pathBasename } from '@/stores/statusBarStore';
import { recentFiles, syncRecentsFromServer } from '@/lib/recent-files';
import { openFileInEditor } from '@/lib/file-actions';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import { ArrowUp, Bot, FileText, FlaskConical, FolderGit2, History, Sparkles } from '@/lib/icons';

const kbd = 'px-1.5 py-0.5 rounded bg-surface-elevated border border-hairline text-[10px] text-shell-body';

/**
 * The empty center's composer surface: a session starts from HERE — context
 * chips (kiln / project / agent) over a prompt box, model picker in the box's
 * footer, quick actions and recent files below. Nothing touches the daemon
 * until the first message is sent (same lazy-create flow as the draft
 * surface); the created chat docks right per WS-220 and the center stays the
 * editing surface.
 */
export const CenterComposer: Component = () => {
  const { createSession } = useSessionSafe();

  const [agents, setAgents] = createSignal<AgentProfileEntry[]>([]);
  const [models, setModels] = createSignal<string[]>([]);
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [defaultKiln, setDefaultKiln] = createSignal('');
  const [defaultModel, setDefaultModel] = createSignal('');

  // '' = internal agent / default kiln / default model / no project.
  const [agentName, setAgentName] = createSignal('');
  const [kiln, setKiln] = createSignal('');
  const [model, setModel] = createSignal('');
  const [workspace, setWorkspace] = createSignal('');

  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [cloning, setCloning] = createSignal(false);

  let messageRef: HTMLTextAreaElement | undefined;
  const isAcp = () => agentName() !== '';

  onMount(() => {
    // No barrier: each chip fills as its data arrives. Agents/providers are
    // the slow calls (daemon probes); the splash must not wait on them.
    void getConfig()
      .then((cfg) => cfg?.kiln_path && setDefaultKiln(cfg.kiln_path))
      .catch(() => {});
    void listAgents().then(setAgents).catch(() => {});
    void listAllModels()
      .then((mo) => setModels(mo.filter((m) => !m.startsWith('[error]'))))
      .catch(() => {});
    void listKilns().then(setKilns).catch(() => {});
    void listProjects().then(setProjects).catch(() => {});
    void listProviders()
      .then((providers) => {
        const first = providers.find((p) => p.available);
        if (first?.default_model) setDefaultModel(first.default_model);
      })
      .catch(() => {});
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
        },
        {
          initialMessage: text,
          model: !isAcp() && model() ? model() : undefined,
        },
      );
      setMessage('');
    } catch {
      // Error surfaced via the session context's notification.
    } finally {
      setBusy(false);
    }
  };

  // The default kiln's REGISTERED name (kiln.toml), not its path basename.
  const defaultKilnName = () => {
    const match = kilns().find((k) => k.path === defaultKiln());
    return (
      match?.name ||
      (defaultKiln() ? pathBasename(defaultKiln()) || defaultKiln() : 'Home kiln')
    );
  };

  const kilnOptions = (): ChipOption[] => [
    { value: '', label: defaultKilnName(), hint: 'default' },
    ...kilns()
      .filter((k) => k.path !== defaultKiln())
      .map((k) => ({
        value: k.path,
        label: k.name || pathBasename(k.path) || k.path,
        hint: k.path,
      })),
    // Explicitly kiln-less — a session with no knowledge base attached.
    { value: 'none', label: 'No kiln' },
  ];

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
    return [
      { value: '', label: 'No project' },
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
    { value: '', label: 'Internal agent' },
    ...agents().map((a) => ({
      value: a.name,
      label: a.name,
      hint: a.available ? a.description : 'not installed',
      disabled: !a.available,
    })),
  ];

  const modelOptions = (): ChipOption[] => [
    {
      value: '',
      label: defaultModel() ? `Auto (${defaultModel()})` : 'Auto',
    },
    ...models().map((m) => ({ value: m, label: m })),
  ];

  const openNotesPalette = () =>
    window.dispatchEvent(
      new CustomEvent('crucible:open-command-palette', { detail: { mode: 'notes' } }),
    );
  const openCommandsPalette = () =>
    window.dispatchEvent(new CustomEvent('crucible:open-command-palette'));

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
            />
            <ChipSelect
              name="agent"
              icon={Bot}
              options={agentOptions()}
              value={agentName()}
              onSelect={setAgentName}
              disabled={busy()}
              testid="composer-agent"
            />
          </div>

          {/* The composer card. */}
          <div class="bg-surface-base border border-hairline-strong rounded-xl px-3 pt-2 pb-2 focus-within:border-primary transition-colors shadow-lg">
            <textarea
              ref={messageRef}
              value={message()}
              onInput={(e) => setMessage(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  void submit();
                }
              }}
              placeholder="Plan, build, ask — a session starts with your first message"
              aria-label="First message"
              rows={3}
              class="w-full bg-transparent text-sm text-shell-ink placeholder-muted-dark resize-none outline-none px-1 py-1 min-h-[3.5rem]"
              data-testid="composer-input"
            />
            <div class="flex items-center gap-1">
              <Show when={!isAcp()}>
                <ChipSelect
                  name="model"
                  icon={Sparkles}
                  options={modelOptions()}
                  value={model()}
                  onSelect={setModel}
                  disabled={busy()}
                  testid="composer-model"
                />
              </Show>
              <button
                type="button"
                onClick={() => void submit()}
                disabled={!message().trim()}
                aria-label="Start session"
                title="Start session (Enter)"
                classList={{
                  'ml-auto w-7 h-7 rounded-full flex items-center justify-center transition-colors': true,
                  'bg-primary text-white hover:bg-primary-hover': !!message().trim(),
                  'bg-surface-elevated text-muted-dark cursor-not-allowed': !message().trim(),
                }}
                data-testid="composer-send"
              >
                <ArrowUp class="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Quick actions — do, don't describe. */}
          <div class="mt-4 flex items-center justify-center gap-6 text-xs text-muted">
            <button
              type="button"
              class="flex items-center gap-2 hover:text-shell-ink transition-colors"
              onClick={openNotesPalette}
              data-testid="cta-open-file"
            >
              <FileText class="w-3.5 h-3.5" />
              Open a file
              <span class={kbd}>Ctrl+O</span>
            </button>
            <button
              type="button"
              class="flex items-center gap-2 hover:text-shell-ink transition-colors"
              onClick={openCommandsPalette}
              data-testid="cta-commands"
            >
              Commands
              <span class={kbd}>Ctrl+P</span>
            </button>
          </div>

          {/* Pick up where you left off. */}
          <Show when={recentFiles().length > 0}>
            <div class="mt-2 flex flex-col items-center gap-0.5" data-testid="composer-recents">
              <For each={recentFiles().slice(0, 5)}>
                {(r) => (
                  <button
                    type="button"
                    class="flex items-center gap-2 px-2 py-1 rounded text-xs text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors max-w-full"
                    onClick={() => openFileInEditor(r.absPath, r.name)}
                    title={r.absPath}
                  >
                    <History class="w-3 h-3 flex-shrink-0 text-muted-dark" />
                    <span class="truncate">{r.name}</span>
                    <span class="text-muted-dark truncate max-w-[240px]">{r.absPath}</span>
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};
