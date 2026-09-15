// src/components/settings/ModelSettings.tsx
//
// The model section: the settings the DAEMON says this session supports, plus
// the settings an external agent declares for itself.
import { Component, Show, For, createSignal, onMount } from 'solid-js';
import { Brain } from '@/lib/icons';

import { SettingRow, SettingsSectionState } from './primitives';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { AgentConfigOption } from '@/lib/types';
import {
  getPrecognition,
  setPrecognition as apiSetPrecognition,
  listKnobs,
  listAgentOptions,
  setAgentOption as apiSetAgentOption,
} from '@/lib/api';

export const ModelSettingsSection: Component = () => {
  const session = useSessionSafe();

  const [precognition, setPrecognition] = createSignal(true);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  /**
   * Which settings this session actually has.
   *
   * Empty until the daemon answers, and a control is drawn only once it says
   * so. An ACP session runs its own turn loop, so the daemon's caps and
   * context policy describe work it does not do, and the daemon refuses
   * those settings outright.
   *
   * Defaulting to "hidden" rather than "shown" is deliberate: a control that
   * appears and then errors is worse than one that appears a moment late.
   */
  const [supported, setSupported] = createSignal<Set<string>>(new Set());
  const has = (id: string) => supported().has(id);
  /**
   * The settings the external agent advertised for itself.
   *
   * Not Crucible's, and not a fixed list: a reasoning-level selector, a
   * toggle the agent invented. Empty for an internal agent and until the
   * first message, because an agent says what it has when the daemon
   * connects to it.
   */
  const [agentOptions, setAgentOptions] = createSignal<AgentConfigOption[]>([]);

  const loadSettings = async () => {
    const s = session.currentSession();
    if (!s) {
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const [knobs, agentOpts, precog] = await Promise.all([
        listKnobs(s.id),
        // An older daemon has no such method; an empty list is the right
        // answer there, and is what an internal session gives anyway.
        listAgentOptions(s.id).catch(() => ({ options: [] as AgentConfigOption[] })),
        getPrecognition(s.id),
      ]);
      setSupported(new Set(knobs.knobs.filter((k) => k.supported).map((k) => k.id)));
      setAgentOptions(agentOpts.options);
      setPrecognition(precog);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load settings');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadSettings);

  const inputClass = 'bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus-ring';

  /**
   * Send one of the agent's own settings back to it.
   *
   * The agent is the only authority on what the value became — it may clamp
   * or normalise what it is sent — so the list is re-read rather than
   * updated optimistically.
   */
  const handleAgentOption = async (option: AgentConfigOption, value: string) => {
    const s = session.currentSession();
    if (!s) return;
    try {
      await apiSetAgentOption(s.id, option.id, value);
      const fresh = await listAgentOptions(s.id);
      setAgentOptions(fresh.options);
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to set ${option.name}`);
    }
  };

  const handlePrecognitionToggle = async () => {
    const s = session.currentSession();
    if (!s) return;

    const newVal = !precognition();
    setPrecognition(newVal);
    try {
      await apiSetPrecognition(s.id, newVal);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set precognition');
      setPrecognition(!newVal); // revert
    }
  };

  return (
    <SettingsSectionState
      title="Model Settings"
      icon={Brain}
      loading={loading()}
      error={error()}
      loadingMessage="Loading settings…"
      requiresSession
      hasSession={!!session.currentSession()}
      noSessionMessage="No active session — start a chat to configure model settings."
    >
      <Show when={has('precognition')}>
      <SettingRow label="Precognition" description="Auto-inject context">
        <button
          onClick={handlePrecognitionToggle}
          data-testid="precognition-toggle"
          class={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
            precognition() ? 'bg-primary' : 'bg-muted-dark'
          }`}
        >
          <span
            class={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              precognition() ? 'translate-x-6' : 'translate-x-1'
            }`}
          />
        </button>
      </SettingRow>
      </Show>


      {/*
        The external agent's own settings. Crucible has no knob for these and
        does not interpret them: the agent said it has a `thought_level`
        selector, so one is drawn. A different agent lists different things,
        which is why this is a loop and not a set of named rows.
      */}
      <For each={agentOptions()}>
        {(option) => (
          <SettingRow label={option.name} description={option.description ?? undefined}>
            <Show
              when={option.kind === 'select'}
              fallback={
                <button
                  onClick={() => handleAgentOption(option, String(!option.current))}
                  data-testid={`agent-option-${option.id}`}
                  class={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                    option.current ? 'bg-primary' : 'bg-muted-dark'
                  }`}
                >
                  <span
                    class={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                      option.current ? 'translate-x-6' : 'translate-x-1'
                    }`}
                  />
                </button>
              }
            >
              <select
                value={String(option.current)}
                onChange={(e) => handleAgentOption(option, e.currentTarget.value)}
                data-testid={`agent-option-${option.id}`}
                class={`${inputClass} w-40`}
              >
                <For each={option.choices ?? []}>
                  {(choice) => <option value={choice.value}>{choice.name}</option>}
                </For>
              </select>
            </Show>
          </SettingRow>
        )}
      </For>
    </SettingsSectionState>
  );
};
