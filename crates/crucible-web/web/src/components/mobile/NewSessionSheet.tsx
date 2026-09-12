import { Component, For, Show, createMemo, createSignal, onMount } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { useSessionSafe } from '@/contexts/SessionContext';
import { BottomSheet, SheetOption } from '@/components/mobile/BottomSheet';
import { closeDraftTab } from '@/lib/draft-session';
import { draftCreateParams, HOST_RUNTIME } from '@/lib/session-draft';
import { iconForAgent } from '@/lib/agent-icons';
import { kilnNameForPath } from '@/lib/kiln-registry';
import {
  getConfig,
  getProviderTargets,
  getTargetProviders,
  listAgents,
  listAllModels,
  listKilns,
  listProjects,
  type ProviderTarget,
  type TargetProvider,
} from '@/lib/api';
import type { AgentProfileEntry, KilnListEntry, Project } from '@/lib/types';
import { ChevronRight } from '@/lib/icons';

type Step = 'agent' | 'context' | 'prompt';

/** One axis the user may set. '' always means "untouched", never "none". */
interface Axis {
  id: string;
  label: string;
  value: () => string;
  shown: () => string;
  options: () => { value: string; label: string }[];
  set: (value: string) => void;
}

/**
 * Starting a session on a phone: the agent, then the context, then the prompt.
 *
 * The desktop composer puts five chips and a model picker on one row, which
 * needs width a phone has not. The steps carry the same five axes and the same
 * empty-value contract (`lib/session-draft.ts`), so neither surface can drift
 * into creating a session the other could not.
 *
 * Step one is also the only screen that says what each agent IS: a chip has
 * room for a name and nothing else.
 */
export const NewSessionSheet: Component<{ draftTabId?: string; workspace?: string }> = (props) => {
  const { createSession } = useSessionSafe();
  const [step, setStep] = createSignal<Step>('agent');
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal('');
  const [picking, setPicking] = createSignal<Axis | null>(null);

  const [agents, setAgents] = createSignal<AgentProfileEntry[]>([]);
  const [models, setModels] = createSignal<string[]>([]);
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [defaultKilnPath, setDefaultKilnPath] = createSignal('');
  const [wsProviders, setWsProviders] = createSignal<TargetProvider[]>([]);
  const [rtProviders, setRtProviders] = createSignal<TargetProvider[]>([]);
  const [wsTargets, setWsTargets] = createSignal<ProviderTarget[]>([]);
  const [rtTargets, setRtTargets] = createSignal<ProviderTarget[]>([]);

  const [agentName, setAgentName] = createSignal('');
  const [kiln, setKiln] = createSignal('');
  const [model, setModel] = createSignal('');
  const [workspace, setWorkspace] = createSignal(props.workspace ?? '');
  const [wsTarget, setWsTarget] = createSignal('');
  const [runtime, setRuntime] = createSignal('');

  onMount(() => {
    void getConfig().then((c) => setDefaultKilnPath(c?.kiln_path ?? '')).catch(() => {});
    void listAgents().then(setAgents).catch(() => {});
    void listAllModels().then(setModels).catch(() => {});
    void listKilns().then(setKilns).catch(() => {});
    void listProjects().then(setProjects).catch(() => {});
    void getTargetProviders('workspace').then(setWsProviders).catch(() => {});
    void getTargetProviders('runtime').then(setRtProviders).catch(() => {});
  });

  // Targets are per provider; the sheet shows them flat, spec and all.
  const loadTargets = async (
    providers: TargetProvider[],
    set: (t: ProviderTarget[]) => void,
  ) => {
    const all: ProviderTarget[] = [];
    for (const provider of providers) {
      try {
        all.push(...(await getProviderTargets(provider, workspace() || undefined)));
      } catch {
        /* a provider that cannot list offers nothing */
      }
    }
    set(all);
  };
  onMount(() => {
    void Promise.resolve().then(async () => {
      await loadTargets(wsProviders(), setWsTargets);
      await loadTargets(rtProviders(), setRtTargets);
    });
  });

  const defaultKilnName = () => kilnNameForPath(defaultKilnPath(), kilns());
  const labelFor = (options: { value: string; label: string }[], value: string, fallback: string) =>
    options.find((o) => o.value === value)?.label ?? fallback;

  const axes = createMemo<Axis[]>(() => {
    const projectOptions = [
      { value: '', label: 'Unset' },
      ...projects().map((p) => ({ value: p.path, label: p.name || p.path })),
    ];
    const kilnOptions = [
      { value: '', label: defaultKilnName() ? `Default (${defaultKilnName()})` : 'Default' },
      { value: 'none', label: 'No kiln' },
      ...kilns()
        .filter((k) => !!k.name?.trim())
        .map((k) => ({ value: k.name!, label: k.name! })),
    ];
    const modelOptions = [
      { value: '', label: 'Default' },
      ...models().map((m) => ({ value: m, label: m })),
    ];
    const wsOptions = [
      { value: '', label: 'This checkout' },
      ...wsTargets().map((t) => ({ value: t.spec, label: t.label })),
    ];
    const rtOptions = [
      { value: '', label: "The project's setting" },
      { value: HOST_RUNTIME, label: 'This machine' },
      ...rtTargets().map((t) => ({ value: t.spec, label: t.label })),
    ];
    return [
      { id: 'project', label: 'Project', value: workspace, options: () => projectOptions,
        shown: () => labelFor(projectOptions, workspace(), 'Unset'), set: setWorkspace },
      { id: 'workspace', label: 'Workspace', value: wsTarget, options: () => wsOptions,
        shown: () => labelFor(wsOptions, wsTarget(), 'This checkout'), set: setWsTarget },
      { id: 'kiln', label: 'Kiln', value: kiln, options: () => kilnOptions,
        shown: () => labelFor(kilnOptions, kiln(), 'Default'), set: setKiln },
      { id: 'model', label: 'Model', value: model, options: () => modelOptions,
        shown: () => labelFor(modelOptions, model(), 'Default'), set: setModel },
      { id: 'runtime', label: 'Runtime', value: runtime, options: () => rtOptions,
        shown: () => labelFor(rtOptions, runtime(), "The project's setting"), set: setRuntime },
    ];
  });

  const send = async () => {
    const text = message().trim();
    if (!text || busy()) return;
    setBusy(true);
    try {
      await createSession(
        draftCreateParams({
          kiln: kiln(),
          defaultKiln: defaultKilnName(),
          workspace: workspace(),
          agentName: agentName(),
          wsTarget: wsTarget(),
          runtime: runtime(),
        }),
        { initialMessage: text, model: !agentName() && model() ? model() : undefined },
      );
      setMessage('');
      if (props.draftTabId) closeDraftTab(props.draftTabId);
    } catch {
      // The session context raises the error as a notification.
    } finally {
      setBusy(false);
    }
  };

  const stepTitle = () => (step() === 'agent' ? 'Agent' : step() === 'context' ? 'Context' : 'Message');

  return (
    <div class="flex-1 min-h-0 flex flex-col">
      <header class="shrink-0 flex items-center gap-2 px-3 h-11 border-b border-hairline">
        <h2 class="flex-1 text-sm font-medium text-shell-ink">{stepTitle()}</h2>
        <Show when={step() !== 'agent'}>
          <button
            type="button"
            class="h-11 px-3 text-sm rounded text-muted-dark hover:text-shell-ink focus-ring"
            onClick={() => setStep(step() === 'prompt' ? 'context' : 'agent')}
          >
            Back
          </button>
        </Show>
        <Show when={step() !== 'prompt'}>
          <button
            type="button"
            class="h-11 px-3 text-sm font-medium rounded text-shell-ink hover:bg-hover-wash focus-ring"
            onClick={() => setStep(step() === 'agent' ? 'context' : 'prompt')}
          >
            Next
          </button>
        </Show>
      </header>

      <div class="flex-1 min-h-0 overflow-y-auto">
        <Show when={step() === 'agent'}>
          <div role="radiogroup" aria-label="Agent" class="flex flex-col gap-1 p-2">
            <For each={[{ name: '', description: 'The built-in agent, grounded in your kilns.' }, ...agents()]}>
              {(agent) => (
                <button
                  type="button"
                  role="radio"
                  aria-checked={agentName() === agent.name}
                  class={`flex items-start gap-3 p-3 rounded text-left focus-ring ${
                    agentName() === agent.name ? 'bg-control' : 'hover:bg-hover-wash'
                  }`}
                  onClick={() => setAgentName(agent.name)}
                >
                  <Dynamic component={iconForAgent(agent.name)} class="w-5 h-5 mt-0.5 shrink-0 text-muted-dark" />
                  <span class="min-w-0">
                    <span class="block text-sm font-medium text-shell-ink">
                      {agent.name || 'Crucible'}
                    </span>
                    <span class="block text-xs text-muted-dark">
                      {agent.description || 'An ACP agent.'}
                    </span>
                  </span>
                </button>
              )}
            </For>
          </div>
        </Show>

        <Show when={step() === 'context'}>
          <div class="flex flex-col">
            <For each={axes()}>
              {(axis) => (
                <button
                  type="button"
                  aria-label={`${axis.label}: ${axis.shown()}`}
                  class="h-11 px-4 flex items-center gap-2 text-left text-sm hover:bg-hover-wash focus-ring"
                  onClick={() => setPicking(axis)}
                >
                  <span class="w-24 shrink-0 text-muted-dark">{axis.label}</span>
                  <span class="flex-1 truncate text-shell-ink">{axis.shown()}</span>
                  <ChevronRight class="w-4 h-4 shrink-0 text-muted-dark" />
                </button>
              )}
            </For>
          </div>
        </Show>

        <Show when={step() === 'prompt'}>
          <div class="flex flex-col h-full p-2 gap-2">
            <textarea
              aria-label="Message"
              class="flex-1 min-h-40 w-full resize-none rounded border border-hairline bg-surface-base p-3 text-sm text-shell-ink focus-ring"
              placeholder="What do you want to do?"
              value={message()}
              onInput={(e) => setMessage(e.currentTarget.value)}
            />
            <button
              type="button"
              disabled={!message().trim() || busy()}
              class="h-11 rounded bg-primary text-on-primary text-sm font-medium disabled:opacity-50 focus-ring"
              onClick={() => void send()}
            >
              Send
            </button>
          </div>
        </Show>
      </div>

      <BottomSheet
        open={picking() !== null}
        label={picking()?.label ?? ''}
        onClose={() => setPicking(null)}
      >
        <For each={picking()?.options() ?? []}>
          {(option) => (
            <SheetOption
              label={option.label}
              selected={picking()?.value() === option.value}
              onSelect={() => {
                picking()?.set(option.value);
                setPicking(null);
              }}
            />
          )}
        </For>
      </BottomSheet>
    </div>
  );
};
