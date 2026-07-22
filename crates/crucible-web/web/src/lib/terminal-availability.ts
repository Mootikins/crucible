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
let started = false;
function ensureStarted(): void {
  if (started) return;
  started = true;
  if (isLocalhost()) {
    setRemoteShell(true);
    return;
  }
  try {
    getConfig()
      .then((c) => setRemoteShell(c.remote_shell === true))
      .catch(() => setRemoteShell(false));
  } catch {
    setRemoteShell(false);
  }
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
