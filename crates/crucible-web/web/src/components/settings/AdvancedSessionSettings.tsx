// src/components/settings/AdvancedSessionSettings.tsx
//
// The session config knobs the daemon advertises that do not belong in the
// model panel: the context strategy.
//
// Its own file rather than a tenth section inside one settings module, which
// was already 961 lines — the same reason the Rust routes became
// `routes/session_config/`.
//
// Gate A2e (`crucible-cli/tests/architecture_tests.rs`) proves each knob has a
// backend route; `routes/session_config/tests.rs` proves each route round-trips
// its value under the daemon's field name. This file is the last leg: without it
// the API is wider than the UI, which is reachable-but-unreachable.
import { Component, createSignal } from 'solid-js';

import { Sliders } from '@/lib/icons';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useGetContextStrategy, useSetContextStrategy } from '@/lib/query/session-config';

import { SettingRow, SettingsSectionState } from './primitives';

const inputClass =
  'bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus-ring';

/**
 * The strategy names offered in the dropdown.
 *
 * A convenience list, NOT a validator: the daemon parses the string and answers
 * 422 for one it does not know, so nothing here rejects a value. A `<select>`
 * that silently dropped an option the daemon accepts would be worse than a text
 * field, which is why the current value is always added to the list if it is not
 * already in it.
 */
const CONTEXT_STRATEGIES = ['truncate', 'summarize'];

export const AdvancedSessionSettingsSection: Component = () => {
  const session = useSessionSafe();

  const sessionId = () => session.currentSession()?.session_id ?? null;
  // Keyed by session, so the panel follows the selection instead of holding
  // the strategy of whichever session it was opened on.
  const strategyQuery = useGetContextStrategy(sessionId);
  const contextStrategy = () => strategyQuery.data ?? '';
  const setStrategy = useSetContextStrategy();

  /** The failure of one write, which is not the failure of the read. */
  const [writeError, setWriteError] = createSignal<string | null>(null);
  const error = () => writeError() ?? strategyQuery.error?.message ?? null;

  const options = (known: string[], current: string) =>
    current && !known.includes(current) ? [current, ...known] : known;

  return (
    <SettingsSectionState
      title="Advanced Session"
      icon={Sliders}
      loading={strategyQuery.isLoading}
      error={error()}
      loadingMessage="Loading advanced settings…"
      requiresSession
      hasSession={!!session.currentSession()}
      noSessionMessage="No active session — start a chat to configure advanced settings."
    >
      <SettingRow label="Context Strategy" description="How history is assembled">
        <select
          value={contextStrategy()}
          data-testid="context-strategy-select"
          onChange={async (e) => {
            const val = (e.target as HTMLSelectElement).value;
            const id = sessionId();
            if (!id) return;
            setWriteError(null);
            try {
              // The hook holds the new value and re-reads it: the daemon may
              // store a name other than the one this dropdown sent.
              await setStrategy.mutateAsync({ id, strategy: val });
            } catch (err) {
              setWriteError(
                err instanceof Error ? err.message : 'Failed to set context strategy',
              );
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
