/**
 * Whether the terminal (a full PTY — shell access) is usable from THIS
 * client. Localhost always is; remote clients need the server's
 * `remote_shell` opt-in (reported by /api/config, fail-closed while the
 * check is in flight). UI entry points (ribbon icon, palette command)
 * consult this so a disabled terminal is greyed out / absent instead of
 * present-but-refusing; TerminalPanel itself keeps the honest explanation
 * for anything that still reaches it.
 */
import { createSignal } from 'solid-js';
import { getConfig } from './api';

export function isLocalhost(): boolean {
  const h = window.location.hostname;
  return h === 'localhost' || h === '127.0.0.1' || h === '[::1]' || h === '::1';
}

// true = allowed, false = denied, undefined = remote check still in flight.
const [remoteShell, setRemoteShell] = createSignal<boolean | undefined>(undefined);

// Lazy on first read, NOT at import: a module-level fetch would fire as an
// import side effect in any test/tool that transitively pulls this in under
// a non-localhost URL.
// A promise while a check is in flight, not a boolean latch. The latch made a
// TRANSPORT failure permanent: on a LAN page load the first `/api/config` is a
// 401 (the API group is behind bearer auth), which collapsed *unknown* onto
// *denied* for the lifetime of the page. The terminal then claimed to be
// localhost-only until a full reload, even after signing in.
let inFlight: Promise<void> | null = null;
// These accessors are read from render paths, so "retry when the answer is
// unknown" has to be throttled or every read fires a request. Same shape as the
// auth-prompt throttle in `api.ts`.
const RETRY_COOLDOWN_MS = 3_000;
let lastAttempt = 0;

function ensureStarted(): void {
  if (isLocalhost()) {
    setRemoteShell(true);
    return;
  }
  // Settled either way, or already asking: nothing to do. `undefined` means we
  // never got an answer, so that one is worth re-asking — behind the cooldown.
  if (inFlight || remoteShell() !== undefined) return;
  const now = Date.now();
  if (now - lastAttempt < RETRY_COOLDOWN_MS) return;
  lastAttempt = now;
  try {
    inFlight = getConfig()
      .then((c) => void setRemoteShell(c.remote_shell === true))
      // `undefined`, never `false`: we did not learn that the terminal is
      // denied, we learned nothing. Only `remote_shell === false` is a denial.
      .catch(() => setRemoteShell(undefined))
      .finally(() => {
        inFlight = null;
      });
  } catch {
    setRemoteShell(undefined);
    inFlight = null;
  }
}

// Signing in is the event that turns the 401 above into an answer. Without
// this, a user who dismissed the token prompt and authenticated later — or in
// another tab — kept a terminal that refused for no stated reason.
if (typeof window !== 'undefined') {
  window.addEventListener('crucible:auth-ok', () => {
    setRemoteShell(undefined);
    inFlight = null;
    // Signing in is new information, so it bypasses the cooldown rather than
    // waiting it out.
    lastAttempt = 0;
    ensureStarted();
  });
}

/** Terminal is usable from this client. */
export const terminalAllowed = () => {
  ensureStarted();
  return remoteShell() === true;
};

/** The check finished and the answer is no (distinct from still-loading). */
export const terminalDenied = () => {
  ensureStarted();
  return remoteShell() === false;
};
