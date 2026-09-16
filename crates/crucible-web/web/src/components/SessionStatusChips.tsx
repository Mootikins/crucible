import { Component, For, Show } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useSessionModes } from '@/lib/query/modes';
import { useSessionStatus } from '@/lib/query/session-config';
import { reviewStore, useReviewSession } from '@/lib/review-store';
import {
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


export const SessionStatusChips: Component = () => {
  const { currentSession } = useSessionSafe();

  const sessionId = () => currentSession()?.session_id;
  useReviewSession(sessionId);
  // The chat pane reads this same list for its mode control. This component
  // used to fetch a second copy of it on every mount, and the two disagreed
  // the moment one of them failed.
  const modes = useSessionModes(() => sessionId() ?? null);

  // No SSE event carries plugin status, so the read hangs off the session id.
  // Scoped to the ACTIVE session rather than polling every open one: this is a
  // per-session daemon round trip. The key carries that id, so the previous
  // session's chips cannot linger over a new one while its read is in flight,
  // and a refused read is no chips rather than a notification.
  const status = useSessionStatus(() => sessionId() ?? null);
  const slots = () => status.data ?? [];

  // The EFFECTIVE review policy, straight from the daemon's mode descriptor.
  //
  // Never re-derived from the mode id here. The daemon degrades the configured
  // policy by what the agent can actually enforce — an ACP agent runs its tools
  // in its own process, so a pre-write gate on it is unenforceable and comes
  // back as `post_turn`. A chip reading "gated" on a session that cannot gate
  // is a lie about a safety property, and inferring it client-side is exactly
  // how that lie gets told.
  //
  // It reads the query rather than holding a copy, so a session with no answer
  // yet — a new one, or one whose list failed — reports no policy instead of
  // the policy of the session before it.
  const policy = (): ReviewPolicy | null => {
    const listed = modes.data;
    if (!listed) return null;
    const current = (listed.modes as ReviewAwareMode[]).find(
      (mode) => mode.id === listed.current_mode_id,
    );
    // Absent on daemons that predate the feature — no chip rather than a
    // guessed one.
    return current?.review_policy ?? null;
  };

  const gate = () => reviewStore.session(sessionId()).gate;
  const blocked = () => gate()?.blocked === true;
  // The review policy is no longer a chip: a reader did not know what
  // "gated" meant, and the permission card already says when a write waits.
  // It stays on the wrapper as data for tests and plugins.
  const anything = () => slots().length > 0 || blocked() || policy() !== null;

  return (
    <Show when={anything()}>
      <div class="contents" data-testid="session-status" data-review-policy={policy() ?? undefined}>
        {/* A blocked agent must never read as a stalled one. First chip,
            loudest tone, and it names what it is waiting on. */}
        <Show when={blocked()}>
          <span
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-md border text-floor border-attention/40 bg-attention/10 text-attention"
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

        <For each={slots()}>
          {(slot) => (
            <span
              class={`inline-flex items-center px-2 py-0.5 rounded-md border text-floor ${
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
