/**
 * Reconnect delay for the terminal WebSocket.
 *
 * Its own module because the interesting part is arithmetic, and the component
 * that uses it cannot be mounted honestly in jsdom (xterm needs canvas/WebGL).
 * Extracting this is what makes the policy testable without a mock of a mock.
 *
 * Jittered, unlike the two hand-rolled `EventSource` backoffs in `api.ts`: every
 * client reconnecting on the same schedule after a server restart is a
 * thundering herd, and a terminal restart is exactly when several tabs wake at
 * once. Capped low (15s, versus their 30s) because a terminal is interactive —
 * a shell that takes half a minute to come back reads as broken.
 */

/** Base delay for the first retry, doubled per attempt. */
const BASE_MS = 500;
/** Ceiling before jitter. */
const CAP_MS = 15_000;

/**
 * Delay before retry number `attempt` (0-based).
 *
 * `random` is injectable so the jitter can be pinned in tests; it must return
 * `[0, 1)` like `Math.random`. The result is in `[0.5, 1.0]` of the capped
 * exponential — jitter only ever shortens, so the cap stays a real ceiling.
 */
export function nextReconnectDelay(attempt: number, random: () => number = Math.random): number {
  const exponential = BASE_MS * 2 ** Math.max(0, attempt);
  const capped = Math.min(exponential, CAP_MS);
  return Math.round(capped * (0.5 + random() * 0.5));
}

export const RECONNECT_CAP_MS = CAP_MS;
