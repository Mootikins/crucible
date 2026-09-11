// src/components/settings/AdvancedSessionSettings.tsx
//
// The session config knobs the daemon advertises that do not belong in the
// model panel: context budget and context strategy.
//
// Its own file rather than a tenth section inside SettingsPanel.tsx, which was
// already 961 lines — the same reason the Rust routes became
// `routes/session_config/`.
//
// Gate A2e (`crucible-cli/tests/architecture_tests.rs`) proves each knob has a
// backend route; `routes/session_config/tests.rs` proves each route round-trips
// its value under the daemon's field name. This file is the last leg: without it
// the API is wider than the UI, which is reachable-but-unreachable.
import { Component, createSignal, onMount } from 'solid-js';

import { Sliders } from '@/lib/icons';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getContextBudget,
  getContextStrategy,
  setContextBudget,
  setContextStrategy,
} from '@/lib/api';

import { SettingRow, SettingsSectionState } from './primitives';

const inputClass =
  'bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus:outline-none';

/**
 * The strategy names offered in the dropdown.
 *
 * A convenience list, NOT a validator: the daemon parses the string and answers
 * 422 for one it does not know, so nothing here rejects a value. A `<select>`
 * that silently dropped an option the daemon accepts would be worse than a text
 * field, which is why the current value is always added to the list if it is not
 * already in it.
 */
const CONTEXT_STRATEGIES = ['full', 'recent', 'truncate', 'summarize'];

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
  const [contextStrategy, setContextStrategySig] = createSignal('');
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  const fail = (what: string) => (err: unknown) =>
    setError(err instanceof Error ? err.message : `Failed to set ${what}`);


  const loadSettings = async () => {
    const s = session.currentSession();
    if (!s) {
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [budget, strategy] =
        await Promise.all([
          getContextBudget(s.id),
          getContextStrategy(s.id),
        ]);
      const text = (v: number | null) => (v === null ? '' : String(v));
      setContextBudgetSig(text(budget));
      setContextStrategySig(strategy ?? '');
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
          class={`cru-select ${inputClass} w-32`}
        >
          {options(CONTEXT_STRATEGIES, contextStrategy()).map((name) => (
            <option value={name}>{name}</option>
          ))}
        </select>
      </SettingRow>

    </SettingsSectionState>
  );
};
