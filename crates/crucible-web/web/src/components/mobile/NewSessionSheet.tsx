import { Component, For, Show, createMemo, createSignal, onMount } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { useSessionSafe } from '@/contexts/SessionContext';
import { BottomSheet, SheetOption } from '@/components/mobile/BottomSheet';
import { closeDraftTab } from '@/lib/draft-session';
import { draftCreateParams, HOST_RUNTIME } from '@/lib/session-draft';
import { iconForAgent } from '@/lib/agent-icons';
import { attachableKilns, kilnNameForPath } from '@/lib/kiln-registry';
import { listAllModels } from '@/lib/api';
import { useAgents } from '@/lib/query/agents';
import { useKilns } from '@/lib/query/kilns';
import { useConfig } from '@/lib/query/config';
import { useProjects } from '@/lib/query/projects';
import { useAxisTargets } from '@/lib/query/targets';
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

  const [models, setModels] = createSignal<string[]>([]);
  // The desktop composer's roster, read through the same key: opening the
  // sheet after the composer has already probed costs nothing.
  const agentsQuery = useAgents();
  const agents = () => agentsQuery.data ?? [];
  const kilnsQuery = useKilns();
  const kilns = () => kilnsQuery.data ?? [];
  const projectsQuery = useProjects();
  const projects = () => projectsQuery.data ?? [];
  const configQuery = useConfig();
  const defaultKilnPath = () => configQuery.data?.kiln_path ?? '';

  const [agentName, setAgentName] = createSignal('');
  const [kiln, setKiln] = createSignal('');
  const [model, setModel] = createSignal('');
  const [workspace, setWorkspace] = createSignal(props.workspace ?? '');
  const [wsTarget, setWsTarget] = createSignal('');
  const [runtime, setRuntime] = createSignal('');

  // The same two queries the desktop composer reads, keyed by axis, provider
  // and project. The sheet shows them flat, spec and all.
  const wsAxis = useAxisTargets('workspace', () => workspace() || undefined);
  const rtAxis = useAxisTargets('runtime', () => workspace() || undefined);
  const wsTargets = () => Object.values(wsAxis.targets).flat();
  const rtTargets = () => Object.values(rtAxis.targets).flat();

  onMount(() => {
    void listAllModels().then(setModels).catch(() => {});
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
      // The same rule the two desktop pickers use: a row the daemon reports as
      // `registered: false` names a directory `connect_kiln` refuses, so the
      // phone must not offer it either.
      ...attachableKilns(kilns()).map((k) => ({ value: k.name, label: k.name })),
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
      {/* 56 px, matching the app bar: 44 px controls in a 44 px bar have no
          clearance at all, so a filled one reads as a cut-off band. */}
      <header class="shrink-0 flex items-center gap-2 px-3 h-14 border-b border-hairline">
        <h2 class="text-sm font-semibold uppercase tracking-wide text-muted flex-1">{stepTitle()}</h2>
        <Show when={step() !== 'agent'}>
          <button
            type="button"
            class="h-11 px-3 text-xs rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
            onClick={() => setStep(step() === 'prompt' ? 'context' : 'agent')}
          >
            Back
          </button>
        </Show>
        <Show when={step() !== 'prompt'}>
          <button
            type="button"
            class="h-11 px-3 text-xs font-medium rounded text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
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
                  class={`flex items-start gap-3 p-3 rounded text-left transition-colors focus-ring ${
                    agentName() === agent.name
                      ? 'bg-primary/10 text-shell-ink'
                      : 'hover:bg-hover-wash text-shell-body'
                  }`}
                  onClick={() => setAgentName(agent.name)}
                >
                  <Dynamic component={iconForAgent(agent.name)} class="w-5 h-5 mt-0.5 shrink-0 text-muted-dark" />
                  <span class="min-w-0">
                    <span class="text-reading block font-medium text-shell-ink">
                      {agent.name || 'Crucible'}
                    </span>
                    <span class="block text-floor text-muted-dark">
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
                  class="text-reading h-11 px-4 flex items-center gap-2 text-left hover:bg-hover-wash transition-colors focus-ring"
                  onClick={() => setPicking(axis)}
                >
                  <span class="w-24 shrink-0 text-floor uppercase tracking-wide text-muted-dark">{axis.label}</span>
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
              class="text-reading flex-1 min-h-40 w-full resize-none rounded border border-hairline bg-surface-base p-3 text-shell-ink focus-ring"
              placeholder="What do you want to do?"
              value={message()}
              onInput={(e) => setMessage(e.currentTarget.value)}
            />
            <button
              type="button"
              disabled={!message().trim() || busy()}
              class="h-11 rounded bg-primary hover:bg-primary-hover text-on-primary text-xs font-medium transition-colors disabled:opacity-50 focus-ring"
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
