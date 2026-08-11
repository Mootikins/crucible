import { Component, For, Show, createEffect, createSignal, on } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { getSessionStatus, listModes, type SessionStatusSlot } from '@/lib/api';
import { reviewStore, useReviewSession } from '@/lib/review-store';
import {
  REVIEW_POLICY_LABELS,
  type ReviewAwareMode,
  type ReviewPolicy,
} from '@/lib/review-types';

/**
 * Read-only status strip for the current session: whatever keyed slots the
 * daemon's plugins published, rendered as chips, plus the two things about a
 * session that are not any plugin's to say — whether the agent is parked
 * waiting on review, and what the review policy actually is.
 *
 * The plugin half deliberately knows nothing about any particular plugin. A
 * slot arrives as `{key, plugin, text, level}`; this renders `text`, attributes
 * it to `plugin`, and picks a tone from `level`. There is no branch on `key`
 * and no list of known plugins — a plugin shipped tomorrow gets a chip here
 * for free, which is the whole point of the channel. Adding an
 * `if (key === …)` would quietly revoke that.
 *
 * `level` is matched loosely with a fallback because it is the plugin's word,
 * not an enum this file owns.
 */
const TONES: Record<string, string> = {
  warn: 'border-attention/40 bg-attention/10 text-attention',
  warning: 'border-attention/40 bg-attention/10 text-attention',
  error: 'border-error/40 bg-error/10 text-error',
};
const DEFAULT_TONE = 'border-hairline bg-surface-elevated text-muted';

/** Policies worth a chip. `pre_write` and `post_turn` change what happens to
 * the agent; `none` is the absence of a mechanism and needs no badge. */
const POLICY_TONE: Record<ReviewPolicy, string | null> = {
  none: null,
  post_turn: 'border-hairline bg-surface-elevated text-muted',
  pre_write: 'border-precog/40 bg-precog/10 text-precog',
};

export const SessionStatusChips: Component = () => {
  const { currentSession } = useSessionSafe();
  const [slots, setSlots] = createSignal<SessionStatusSlot[]>([]);
  const [policy, setPolicy] = createSignal<ReviewPolicy | null>(null);

  const sessionId = () => currentSession()?.id;
  useReviewSession(sessionId);

  // No SSE event carries plugin status, so the fetch hangs off the session id.
  // Scoped to the ACTIVE session rather than polling every open one: this is a
  // per-session daemon round trip.
  createEffect(
    on(sessionId, (id) => {
      // Clear first: the previous session's chips must never linger over a
      // new one while its fetch is in flight.
      setSlots([]);
      if (!id) return;
      getSessionStatus(id)
        .then((next) => currentSession()?.id === id && setSlots(next))
        // A failed status fetch is "no chips", never a notification. The
        // request fails on every daemon reconnect, and a session with
        // nothing to say is the normal case anyway.
        .catch(() => currentSession()?.id === id && setSlots([]));
    }),
  );

  // The EFFECTIVE review policy, straight from the daemon's mode descriptor.
  //
  // Never re-derived from the mode id here. The daemon degrades the configured
  // policy by what the agent can actually enforce — an ACP agent runs its tools
  // in its own process, so a pre-write gate on it is unenforceable and comes
  // back as `post_turn`. A chip reading "gated" on a session that cannot gate
  // is a lie about a safety property, and inferring it client-side is exactly
  // how that lie gets told.
  createEffect(
    on(sessionId, (id) => {
      setPolicy(null);
      if (!id) return;
      listModes(id)
        .then((modes) => {
          if (currentSession()?.id !== id) return;
          const current = (modes.modes as ReviewAwareMode[]).find(
            (m) => m.id === modes.current_mode_id,
          );
          // Absent on daemons that predate the feature — no chip rather than
          // a guessed one.
          setPolicy(current?.review_policy ?? null);
        })
        .catch(() => currentSession()?.id === id && setPolicy(null));
    }),
  );

  const gate = () => reviewStore.session(sessionId()).gate;
  const blocked = () => gate()?.blocked === true;
  const policyTone = () => {
    const p = policy();
    return p ? POLICY_TONE[p] : null;
  };

  const anything = () => slots().length > 0 || blocked() || !!policyTone();

  return (
    <Show when={anything()}>
      <div class="flex items-center gap-1 flex-wrap" data-testid="session-status">
        {/* A blocked agent must never read as a stalled one. First chip,
            loudest tone, and it names what it is waiting on. */}
        <Show when={blocked()}>
          <span
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-md border text-[11px] border-attention/40 bg-attention/10 text-attention"
            data-testid="session-review-gate"
            title={`${gate()!.tool} is held until ${gate()!.path ?? 'the file it writes'} has no unreviewed changes.`}
          >
            <span class="w-1.5 h-1.5 rounded-full bg-attention animate-pulse" />
            waiting on review
            <Show when={reviewStore.unreviewedCount(sessionId()) > 0}>
              <span class="opacity-70">({reviewStore.unreviewedCount(sessionId())})</span>
            </Show>
          </span>
        </Show>

        <Show when={policyTone()}>
          <span
            class={`inline-flex items-center px-2 py-0.5 rounded-md border text-[11px] ${policyTone()}`}
            data-testid="session-review-policy"
            title="The review policy in force for this session, after any degradation the agent forces."
          >
            {REVIEW_POLICY_LABELS[policy()!]}
          </span>
        </Show>

        <For each={slots()}>
          {(slot) => (
            <span
              class={`inline-flex items-center px-2 py-0.5 rounded-md border text-[11px] ${
                TONES[slot.level] ?? DEFAULT_TONE
              }`}
              title={`${slot.text} — ${slot.plugin}`}
              data-testid={`session-status-${slot.key}`}
            >
              {slot.text}
            </span>
          )}
        </For>
      </div>
    </Show>
  );
};
