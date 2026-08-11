// src/components/settings/AdvancedSessionSettings.tsx
//
// The nine session config knobs the daemon has always advertised and the web
// could not reach: context budget/window, autocompact threshold, iteration cap,
// execution timeout, validation retries, context strategy, output validation and
// the system prompt override.
//
// Its own file rather than a tenth section inside SettingsPanel.tsx, which was
// already 961 lines — the same reason the Rust routes became
// `routes/session_config/`.
//
// Gate A2e (`crucible-cli/tests/architecture_tests.rs`) proves each knob has a
// backend route; `routes/session_config/tests.rs` proves each route round-trips
// its value under the daemon's field name. This file is the last leg: without it
// the API is wider than the UI, which is reachable-but-unreachable.
import { Component, createSignal, onCleanup, onMount } from 'solid-js';

import { Sliders } from '@/lib/icons';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getAutocompactThreshold,
  getContextBudget,
  getContextStrategy,
  getContextWindow,
  getExecutionTimeout,
  getMaxIterations,
  getOutputValidation,
  getSystemPrompt,
  getValidationRetries,
  setAutocompactThreshold,
  setContextBudget,
  setContextStrategy,
  setContextWindow,
  setExecutionTimeout,
  setMaxIterations,
  setOutputValidation,
  setSystemPrompt,
  setValidationRetries,
} from '@/lib/api';

import { createDebounce, SettingRow, SettingsSectionState } from './primitives';

const inputClass =
  'bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus:outline-none';

/**
 * The strategy and validation names offered in the dropdowns.
 *
 * A convenience list, NOT a validator: the daemon parses the string and answers
 * 422 for one it does not know, so nothing here rejects a value. A `<select>`
 * that silently dropped an option the daemon accepts would be worse than a text
 * field, which is why the current value is always added to the list if it is not
 * already in it.
 */
const CONTEXT_STRATEGIES = ['full', 'recent', 'truncate', 'summarize'];
const OUTPUT_VALIDATIONS = ['none', 'lenient', 'strict'];

/** Empty input → `null`, meaning "restore the daemon's default". */
function parseOptionalInt(raw: string): number | null | undefined {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  const val = parseInt(trimmed, 10);
  return Number.isNaN(val) ? undefined : val;
}

export const AdvancedSessionSettingsSection: Component = () => {
  const session = useSessionSafe();

  const [contextBudget, setContextBudgetSig] = createSignal('');
  const [contextWindow, setContextWindowSig] = createSignal('');
  const [autocompact, setAutocompactSig] = createSignal('');
  const [maxIterations, setMaxIterationsSig] = createSignal('');
  const [executionTimeout, setExecutionTimeoutSig] = createSignal('');
  const [validationRetries, setValidationRetriesSig] = createSignal('');
  const [contextStrategy, setContextStrategySig] = createSignal('');
  const [outputValidation, setOutputValidationSig] = createSignal('');
  const [systemPrompt, setSystemPromptSig] = createSignal('');
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  const fail = (what: string) => (err: unknown) =>
    setError(err instanceof Error ? err.message : `Failed to set ${what}`);

  // The system prompt is a textarea, so it debounces rather than firing a PUT
  // per keystroke. The numeric fields commit on blur/change instead.
  const promptDebounce = createDebounce(async (...args: unknown[]) => {
    const [sid, text] = args as [string, string];
    try {
      await setSystemPrompt(sid, text);
    } catch (err) {
      fail('system prompt')(err);
    }
  }, 400);

  onCleanup(() => promptDebounce.cleanup());

  const loadSettings = async () => {
    const s = session.currentSession();
    if (!s) {
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [budget, window, threshold, iterations, timeout, retries, strategy, validation, prompt] =
        await Promise.all([
          getContextBudget(s.id),
          getContextWindow(s.id),
          getAutocompactThreshold(s.id),
          getMaxIterations(s.id),
          getExecutionTimeout(s.id),
          getValidationRetries(s.id),
          getContextStrategy(s.id),
          getOutputValidation(s.id),
          getSystemPrompt(s.id),
        ]);
      const text = (v: number | null) => (v === null ? '' : String(v));
      setContextBudgetSig(text(budget));
      setContextWindowSig(text(window));
      setAutocompactSig(threshold === null ? '' : String(threshold));
      setMaxIterationsSig(text(iterations));
      setExecutionTimeoutSig(text(timeout));
      setValidationRetriesSig(text(retries));
      setContextStrategySig(strategy ?? '');
      setOutputValidationSig(validation ?? '');
      setSystemPromptSig(prompt ?? '');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load advanced settings');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadSettings);

  /** Commit an `Option<number>` knob on blur. An unparseable value is left alone. */
  const commitOptionalInt =
    (
      setter: (raw: string) => void,
      send: (sessionId: string, value: number | null) => Promise<void>,
      what: string,
    ) =>
    async (e: Event) => {
      const raw = (e.target as HTMLInputElement).value;
      setter(raw);
      const s = session.currentSession();
      if (!s) return;
      const val = parseOptionalInt(raw);
      if (val === undefined) return;
      try {
        await send(s.id, val);
      } catch (err) {
        fail(what)(err);
      }
    };

  const options = (known: string[], current: string) =>
    current && !known.includes(current) ? [current, ...known] : known;

  return (
    <SettingsSectionState
      title="Advanced Session"
      icon={Sliders}
      loading={loading()}
      error={error()}
      loadingMessage="Loading advanced settings…"
      requiresSession
      hasSession={!!session.currentSession()}
      noSessionMessage="No active session — start a chat to configure advanced settings."
    >
      <SettingRow label="Context Budget" description="Tokens of history per turn; empty = default">
        <input
          type="number"
          min={0}
          value={contextBudget()}
          data-testid="context-budget-input"
          onInput={(e) => setContextBudgetSig((e.target as HTMLInputElement).value)}
          onBlur={commitOptionalInt(setContextBudgetSig, setContextBudget, 'context budget')}
          class={`${inputClass} w-28 text-right`}
          placeholder="Default"
        />
      </SettingRow>

      <SettingRow label="Context Window" description="Model window override; empty = default">
        <input
          type="number"
          min={0}
          value={contextWindow()}
          data-testid="context-window-input"
          onInput={(e) => setContextWindowSig((e.target as HTMLInputElement).value)}
          onBlur={commitOptionalInt(setContextWindowSig, setContextWindow, 'context window')}
          class={`${inputClass} w-28 text-right`}
          placeholder="Default"
        />
      </SettingRow>

      <SettingRow label="Autocompact Threshold" description="0–1 fraction of the window">
        <input
          type="number"
          min={0}
          max={1}
          step={0.05}
          value={autocompact()}
          data-testid="autocompact-threshold-input"
          onInput={(e) => setAutocompactSig((e.target as HTMLInputElement).value)}
          onBlur={async (e) => {
            const raw = (e.target as HTMLInputElement).value.trim();
            const s = session.currentSession();
            if (!s) return;
            const val = raw === '' ? null : parseFloat(raw);
            if (val !== null && Number.isNaN(val)) return;
            try {
              await setAutocompactThreshold(s.id, val);
            } catch (err) {
              fail('autocompact threshold')(err);
            }
          }}
          class={`${inputClass} w-28 text-right`}
          placeholder="Default"
        />
      </SettingRow>

      <SettingRow label="Max Iterations" description="Agent-loop cap; empty = default">
        <input
          type="number"
          min={1}
          value={maxIterations()}
          data-testid="max-iterations-input"
          onInput={(e) => setMaxIterationsSig((e.target as HTMLInputElement).value)}
          onBlur={commitOptionalInt(setMaxIterationsSig, setMaxIterations, 'max iterations')}
          class={`${inputClass} w-28 text-right`}
          placeholder="Default"
        />
      </SettingRow>

      <SettingRow label="Execution Timeout" description="Seconds per turn; empty = default">
        <input
          type="number"
          min={1}
          value={executionTimeout()}
          data-testid="execution-timeout-input"
          onInput={(e) => setExecutionTimeoutSig((e.target as HTMLInputElement).value)}
          onBlur={commitOptionalInt(setExecutionTimeoutSig, setExecutionTimeout, 'execution timeout')}
          class={`${inputClass} w-28 text-right`}
          placeholder="Default"
        />
      </SettingRow>

      <SettingRow label="Validation Retries" description="Retries after a failed validation">
        <input
          type="number"
          min={0}
          value={validationRetries()}
          data-testid="validation-retries-input"
          onInput={(e) => setValidationRetriesSig((e.target as HTMLInputElement).value)}
          onBlur={async (e) => {
            const raw = (e.target as HTMLInputElement).value.trim();
            const s = session.currentSession();
            // Required, not nullable: the daemon's setter takes a bare u32, so
            // an empty field has nothing to send.
            if (!s || raw === '') return;
            const val = parseInt(raw, 10);
            if (Number.isNaN(val)) return;
            try {
              await setValidationRetries(s.id, val);
            } catch (err) {
              fail('validation retries')(err);
            }
          }}
          class={`${inputClass} w-20 text-right`}
        />
      </SettingRow>

      <SettingRow label="Context Strategy" description="How history is assembled">
        <select
          value={contextStrategy()}
          data-testid="context-strategy-select"
          onChange={async (e) => {
            const val = (e.target as HTMLSelectElement).value;
            setContextStrategySig(val);
            const s = session.currentSession();
            if (!s) return;
            try {
              await setContextStrategy(s.id, val);
            } catch (err) {
              fail('context strategy')(err);
            }
          }}
          class={`${inputClass} w-32`}
        >
          {options(CONTEXT_STRATEGIES, contextStrategy()).map((name) => (
            <option value={name}>{name}</option>
          ))}
        </select>
      </SettingRow>

      <SettingRow label="Output Validation" description="Strictness of response checks">
        <select
          value={outputValidation()}
          data-testid="output-validation-select"
          onChange={async (e) => {
            const val = (e.target as HTMLSelectElement).value;
            setOutputValidationSig(val);
            const s = session.currentSession();
            if (!s) return;
            try {
              await setOutputValidation(s.id, val);
            } catch (err) {
              fail('output validation')(err);
            }
          }}
          class={`${inputClass} w-32`}
        >
          {options(OUTPUT_VALIDATIONS, outputValidation()).map((name) => (
            <option value={name}>{name}</option>
          ))}
        </select>
      </SettingRow>

      <SettingRow
        label="System Prompt"
        description="Overrides the agent's default; empty to clear"
        controlClass="py-3"
      >
        <textarea
          rows={4}
          value={systemPrompt()}
          data-testid="system-prompt-input"
          onInput={(e) => {
            const val = (e.target as HTMLTextAreaElement).value;
            setSystemPromptSig(val);
            const s = session.currentSession();
            if (s) promptDebounce.debounced(s.id, val);
          }}
          class={`${inputClass} w-full font-mono text-xs`}
          placeholder="Agent default"
        />
      </SettingRow>
    </SettingsSectionState>
  );
};
