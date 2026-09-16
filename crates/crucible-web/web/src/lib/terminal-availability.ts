/**
 * Whether the terminal (a full PTY — shell access) is usable from THIS
 * client. Localhost always is; remote clients need the server's
 * `remote_shell` opt-in (reported by /api/config, fail-closed while the
 * check is in flight). UI entry points (ribbon icon, palette command)
 * consult this so a disabled terminal is greyed out / absent instead of
 * present-but-refusing; TerminalPanel itself keeps the honest explanation
 * for anything that still reaches it.
 */
import { configSnapshot, refetchConfig } from '@/lib/query/config';

export function isLocalhost(): boolean {
  const h = window.location.hostname;
  return h === 'localhost' || h === '127.0.0.1' || h === '[::1]' || h === '::1';
}

/**
 * What the daemon said about `remote_shell`: true = allowed, false = denied,
 * undefined = no answer yet.
 *
 * It reads the shared config query rather than fetching. That removes the
 * 3-second retry cooldown this module used to carry: these accessors are read
 * from render paths, and a bare `getConfig()` behind a cooldown was a request
 * every three seconds for the whole life of a page the daemon kept refusing.
 *
 * The new behaviour has NO timer. The first read starts the one config fetch,
 * and the query answers every later read from its cache until `staleTime`
 * expires; the next reader after that refetches. A transport failure is
 * therefore asked again when the config goes stale, not on a loop of its own.
 *
 * `undefined`, never `false`, when the fetch fails: we did not learn that the
 * terminal is denied, we learned nothing. Only `remote_shell === false` is a
 * denial. Collapsing the two made a LAN page load — where the first
 * `/api/config` is a 401, because the API group sits behind bearer auth —
 * claim the terminal was localhost-only until a full reload.
 */
function remoteShell(): boolean | undefined {
  return configSnapshot()?.remote_shell;
}

// Signing in is the event that turns the 401 above into an answer. Without
// this, a user who dismissed the token prompt and authenticated later — or in
// another tab — kept a terminal that refused for no stated reason. It
// invalidates rather than waiting for `staleTime`, because new credentials are
// new information.
if (typeof window !== 'undefined') {
  window.addEventListener('crucible:auth-ok', () => refetchConfig());
}

/** Terminal is usable from this client. */
export const terminalAllowed = () => {
  if (isLocalhost()) return true;
  return remoteShell() === true;
};

/** The check finished and the answer is no (distinct from still-loading). */
export const terminalDenied = () => {
  if (isLocalhost()) return false;
  return remoteShell() === false;
};
