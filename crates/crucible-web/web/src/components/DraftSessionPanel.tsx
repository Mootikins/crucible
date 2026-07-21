import { Component, For, Show, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getConfig,
  listAgents,
  listAllModels,
  listKilns,
  listProjects,
  listProviders,
} from '@/lib/api';
import type { AgentProfileEntry, KilnListEntry, Project } from '@/lib/types';
import { closeDraftTab } from '@/lib/draft-session';
import { pathBasename } from '@/stores/statusBarStore';

/**
 * Draft session surface — the single session-creation path (lazy creation).
 * Nothing touches the daemon until the first message is sent: the controls
 * pick agent (internal or ACP), kiln, model, and optional project workspace;
 * submit creates the session, hands the message to the real ChatProvider via
 * the pending-first-message handoff, and closes this tab.
 */
export const DraftSessionPanel: Component<{ draftTabId?: string }> = (props) => {
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
  const [extraKilns, setExtraKilns] = createSignal<string[]>([]);
  const [model, setModel] = createSignal('');
  const [workspace, setWorkspace] = createSignal('');

  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  let messageRef: HTMLTextAreaElement | undefined;

  const isAcp = () => agentName() !== '';

  onMount(() => {
    // Land the cursor in the message box so the user can start typing at once.
    messageRef?.focus();
    void (async () => {
      const [cfg, ag, mo, ks, ps, providers] = await Promise.all([
        getConfig().catch(() => null),
        listAgents().catch(() => [] as AgentProfileEntry[]),
        listAllModels().catch(() => [] as string[]),
        listKilns().catch(() => [] as KilnListEntry[]),
        listProjects().catch(() => [] as Project[]),
        listProviders().catch(() => []),
      ]);
      if (cfg?.kiln_path) setDefaultKiln(cfg.kiln_path);
      setAgents(ag);
      setModels(mo.filter((m) => !m.startsWith('[error]')));
      setKilns(ks);
      setProjects(ps);
      // What a defaults-only create actually resolves to (first available
      // provider) — shown so "default" isn't a blank promise.
      const first = providers.find((p) => p.available);
      if (first?.default_model) setDefaultModel(first.default_model);
    })();
  });

  const submit = async () => {
    const text = message().trim();
    if (!text || busy()) return;
    setBusy(true);
    try {
      await createSession(
        {
          kiln: kiln() || defaultKiln() || undefined,
          connect_kilns: extraKilns().length > 0 ? extraKilns() : undefined,
          workspace: workspace() || undefined,
          ...(isAcp() ? { agent_type: 'acp', agent_name: agentName() } : {}),
        },
        {
          initialMessage: text,
          // ACP agents own their model choice; internal defaults resolve
          // server-side when no model is picked.
          model: !isAcp() && model() ? model() : undefined,
        },
      );
      if (props.draftTabId) closeDraftTab(props.draftTabId);
    } catch {
      // Error already surfaced via the session context's notification.
    } finally {
      // Always clear busy: on success closeDraftTab unmounts us, but a
      // draftTabId-less mount (or any failure) must re-enable the button.
      setBusy(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void submit();
    }
  };

  const selectClass =
    'px-2 py-1.5 rounded-lg border border-hairline bg-surface-elevated text-xs text-shell-body outline-none focus:border-primary disabled:opacity-50 max-w-[180px] truncate';

  return (
    <div class="h-full bg-shell-bg flex flex-col items-center justify-center p-6 overflow-y-auto">
      <div class="w-full max-w-xl flex flex-col gap-4">
        <div class="text-center">
          <h2 class="text-lg font-semibold text-shell-ink">New Session</h2>
          <p class="text-xs text-muted mt-1">
            Nothing is created until you send the first message.
          </p>
        </div>

        <div class="flex flex-col gap-2 bg-surface-base rounded-xl p-3 border border-hairline">
          {/* Grouped controls: agent, kiln, model, project (design §7). */}
          <div class="flex items-center gap-2 flex-wrap" data-testid="draft-controls">
            <select
              class={selectClass}
              value={agentName()}
              disabled={busy()}
              onChange={(e) => setAgentName(e.currentTarget.value)}
              title="Agent"
              aria-label="Agent"
              data-testid="draft-agent"
            >
              <option value="">Internal agent</option>
              <For each={agents()}>
                {(a) => (
                  <option value={a.name} disabled={!a.available}>
                    {a.name}
                    {a.description ? ` — ${a.description}` : ''}
                    {a.available ? '' : ' (not installed)'}
                  </option>
                )}
              </For>
            </select>

            <select
              class={selectClass}
              value={kiln()}
              disabled={busy()}
              onChange={(e) => {
                setKiln(e.currentTarget.value);
                // The primary can't also be an extra.
                setExtraKilns((prev) => prev.filter((p) => p !== e.currentTarget.value));
              }}
              title="Kiln (knowledge)"
              aria-label="Kiln (knowledge)"
              data-testid="draft-kiln"
            >
              <option value="">
                {defaultKiln() ? `${pathBasename(defaultKiln())} (default)` : 'Home kiln (default)'}
              </option>
              <For each={kilns().filter((k) => k.path !== defaultKiln())}>
                {(k) => <option value={k.path}>{k.name || pathBasename(k.path)}</option>}
              </For>
            </select>

            {/* Attach additional knowledge kilns (session.connect_kilns). */}
            <select
              class={selectClass}
              value=""
              disabled={busy()}
              onChange={(e) => {
                const path = e.currentTarget.value;
                if (path) setExtraKilns((prev) => [...prev, path]);
                e.currentTarget.value = '';
              }}
              title="Attach more kilns"
              aria-label="Attach more kilns"
              data-testid="draft-extra-kilns"
            >
              <option value="">+ kiln…</option>
              <For
                each={kilns().filter(
                  (k) =>
                    k.path !== (kiln() || defaultKiln()) && !extraKilns().includes(k.path),
                )}
              >
                {(k) => <option value={k.path}>{k.name || pathBasename(k.path)}</option>}
              </For>
            </select>

            <Show when={!isAcp()}>
              <select
                class={selectClass}
                value={model()}
                disabled={busy()}
                onChange={(e) => setModel(e.currentTarget.value)}
                title="Model"
                aria-label="Model"
                data-testid="draft-model"
              >
                <option value="">
                  {defaultModel() ? `${defaultModel()} (default)` : 'Default model'}
                </option>
                <For each={models()}>{(m) => <option value={m}>{m}</option>}</For>
              </select>
            </Show>

            <select
              class={selectClass}
              value={workspace()}
              disabled={busy()}
              onChange={(e) => setWorkspace(e.currentTarget.value)}
              title="Project workspace (optional)"
              aria-label="Project workspace (optional)"
              data-testid="draft-project"
            >
              <option value="">No project</option>
              <For each={projects()}>
                {(p) => <option value={p.path}>{p.name || pathBasename(p.path)}</option>}
              </For>
            </select>
          </div>

          <textarea
            ref={messageRef}
            value={message()}
            onInput={(e) => setMessage(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type your first message..."
            aria-label="First message"
            disabled={busy()}
            rows={3}
            class="w-full bg-transparent text-shell-ink placeholder-muted-dark resize-none outline-none px-2 py-1 min-h-[4rem] disabled:opacity-50"
            data-testid="draft-input"
          />

          <div class="flex items-center justify-end">
            <button
              type="button"
              onClick={() => void submit()}
              disabled={!message().trim() || busy()}
              class="px-3 py-1.5 rounded-lg bg-primary text-sm text-white disabled:opacity-50 disabled:cursor-not-allowed hover:bg-primary-hover transition-colors"
              data-testid="draft-send"
            >
              {busy() ? 'Creating…' : 'Start session'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
