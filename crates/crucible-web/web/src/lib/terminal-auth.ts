/**
 * Whether a failed terminal handshake is a credential problem.
 *
 * A WebSocket `close` event carries no HTTP status, so the one failure the
 * reconnect loop must NEVER retry — the server refusing the credential — is
 * indistinguishable from every drop it must retry. That gap had a specific
 * cost: with `remote_shell` enabled the PTY endpoint demands the API key even
 * from localhost, an unauthenticated page's socket was refused on every
 * attempt, and the loop retried at the backoff cap for the life of the tab —
 * silent, because `authRequired` is emitted by the fetch layer and a socket
 * bypasses the fetch layer. The user saw a terminal that reconnects forever
 * and was never once asked for the key that would fix it.
 *
 * The probe makes the refusal a fact instead of a guess: the endpoint answers
 * a plain (non-upgrade) request with the same 401/403 JSON the auth
 * middleware gives every other gated route, so one `fetch` against the same
 * URL classifies the handshake. `auth` is terminal for the panel — it stops
 * scheduling, names the fault, and opens the sign-in prompt; `authOk` (the
 * sign-in event) is what re-arms it. `transient` belongs to the backoff loop
 * exactly as before.
 */

/** HTTP statuses that mean "the credential, not the connection, is the problem". */
export function isAuthStatus(status: number): boolean {
  return status === 401 || status === 403;
}

/**
 * Ask the terminal endpoint, over plain HTTP, whether it is refusing us.
 *
 * Injectable `doFetch` mirrors `nextReconnectDelay`'s injectable `random`:
 * the decision is the unit under test, not the network.
 */
export async function classifyTerminalAuth(
  url: string,
  doFetch: typeof fetch = globalThis.fetch.bind(globalThis),
): Promise<'auth' | 'transient'> {
  try {
    // `fetch` cannot load `ws(s)://` — the probe rides the same origin in its
    // http form, where the dev proxy and the upgrade route both answer it.
    const httpUrl = url.replace(/^wss?:\/\//, (m) => (m.startsWith('wss') ? 'https://' : 'http://'));
    const response = await doFetch(httpUrl, { method: 'GET' });
    return isAuthStatus(response.status) ? 'auth' : 'transient';
  } catch {
    // The probe itself could not reach the server: that is a DOWN server,
    // which the backoff loop already owns. It is not evidence about keys.
    return 'transient';
  }
}
