// src/components/settings/ModelSettings.tsx
//
// The model section: the settings the DAEMON says this session supports, plus
// the settings an external agent declares for itself.
import { Component, Show, For, createSignal } from 'solid-js';
import { Brain } from '@/lib/icons';

import { SettingRow, SettingsSectionState } from './primitives';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { AgentConfigOption } from '@/lib/types';
import {
  useAgentOptions,
  useGetPrecognition,
  useSessionKnobs,
  useSetAgentOption,
  useSetPrecognition,
} from '@/lib/query/session-config';

export const ModelSettingsSection: Component = () => {
  const session = useSessionSafe();
  const sessionId = () => session.currentSession()?.session_id ?? null;

  /** The failure of one write, which is not the failure of a read. */
  const [writeError, setWriteError] = createSignal<string | null>(null);

  /**
   * Which settings this session actually has.
   *
   * A control is drawn only once the daemon says so. An ACP session runs its
   * own turn loop, so the daemon's caps and context policy describe work it
   * does not do, and the daemon refuses those settings outright.
   *
   * Defaulting to "hidden" rather than "shown" is deliberate: a control that
   * appears and then errors is worse than one that appears a moment late.
   */
  const knobs = useSessionKnobs(sessionId);
  const has = (id: string) =>
    knobs.data?.knobs.some((knob) => knob.id === id && knob.supported) === true;

  /**
   * The settings the external agent advertised for itself.
   *
   * Not Crucible's, and not a fixed list: a reasoning-level selector, a
   * toggle the agent invented. Empty for an internal agent and until the
   * first message, because an agent says what it has when the daemon
   * connects to it.
   */
  const options = useAgentOptions(sessionId);
  const agentOptions = () => options.data?.options ?? [];
  const precognitionQuery = useGetPrecognition(sessionId);
  const precognition = () => precognitionQuery.data !== false;

  const setOption = useSetAgentOption();
  const setPrecognition = useSetPrecognition();

  // The three reads are keyed by session, so the panel reopened on a session
  // it already read paints at once and no longer shows a loading barrier.
  const loading = () =>
    knobs.isLoading || options.isLoading || precognitionQuery.isLoading;
  const error = () =>
    writeError() ??
    knobs.error?.message ??
    precognitionQuery.error?.message ??
    null;

  const inputClass = 'bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus-ring';

  /**
   * Send one of the agent's own settings back to it.
   *
   * The agent is the only authority on what the value became — it may clamp
   * or normalise what it is sent — so the list is re-read rather than
   * updated optimistically. The hook owns that re-read.
   */
  const handleAgentOption = async (option: AgentConfigOption, value: string) => {
    const id = sessionId();
    if (!id) return;
    setWriteError(null);
    try {
      await setOption.mutateAsync({ id, optionId: option.id, value });
    } catch (err) {
      setWriteError(err instanceof Error ? err.message : `Failed to set ${option.name}`);
    }
  };

  const handlePrecognitionToggle = async () => {
    const id = sessionId();
    if (!id) return;
    setWriteError(null);
    try {
      // The hook moves the toggle first and puts it back if the daemon
      // refuses, so nothing here touches the value.
      await setPrecognition.mutateAsync({ id, enabled: !precognition() });
    } catch (err) {
      setWriteError(err instanceof Error ? err.message : 'Failed to set precognition');
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
                <For each={option.kind === 'select' ? option.choices : []}>
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
