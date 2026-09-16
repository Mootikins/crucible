/**
 * The one HTTP client, built from the generated contract.
 *
 * `createClient<paths>` reads `api-schema.d.ts`, which `openapi-typescript`
 * writes from `openapi.json`, which the axum router writes. A call therefore
 * names a path the document carries, a method that path declares, and the
 * parameters and the body that method declares. A typo, a renamed query
 * parameter and a route deleted in Rust all fail `bun run typecheck` instead
 * of reaching a user as `undefined`.
 *
 * The reply type comes from the document too. Nothing in this layer asserts a
 * network shape with `as`, which was the hazard Task A13 removed: one
 * `(await res.json()) as T` stood behind about 90 call sites, so every one of
 * them believed whatever the generic said.
 *
 * `decode` is the second half. `openapi-fetch` answers `{ data, error,
 * response }` and throws nothing, so a caller that read `data` alone would
 * read `undefined` after a refusal. `decode` turns the refusal into the
 * `ApiError` this file has always thrown, carrying the daemon's own sentence.
 */
import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from './api-schema';
import { notificationActions } from '@/stores/notificationStore';

/**
 * The header a caller declares itself in, on the plugin routes.
 *
 * **Not a secret and not a credential.** Any script on this origin can set it,
 * so it stops nothing hostile — the server refuses a request that names
 * *nobody*, which turns "a block reached for another plugin's command" from a
 * silent success into an error someone can read. `routes/plugin_caller.rs`
 * carries the long version; read it before treating this as a gate.
 *
 * It rides on every request rather than only the plugin ones, so a route
 * gated later does not need a second pass over the call sites.
 */
export const PLUGIN_CALLER_HEADER = 'X-Crucible-Plugin';

/** What the app's own UI calls itself. */
export const APP_CALLER = 'app';

/** A failed call, carrying the status a caller branches on. */
export interface ApiError extends Error {
  status: number;
}

/** What one call does about its own failure, beyond throwing. */
export interface FailureOptions {
  /**
   * Raise an error notification carrying the server's own sentence rather
   * than the status alone.
   *
   * For the calls whose callers swallow a failure or fold it into local
   * state — the file tree's first listing, the model and mode lists — where
   * a bare "422" used to be all the user ever saw. The store deduplicates a
   * sentence that repeats within a few seconds, so a burst is one toast.
   */
  notify?: boolean;
}

/**
 * What every path hangs off. Nothing, in a browser.
 *
 * Every call names a root-relative path, which a browser already resolves
 * against this page. `openapi-fetch` builds a `Request` before it calls
 * `fetch`, though, and outside a browser the `Request` constructor refuses a
 * relative URL rather than resolving it — so a test runner needs the origin
 * spelled out. The probe asks which one this is instead of sniffing for
 * jsdom, and the answer is the same URL either way.
 */
function apiBaseUrl(): string {
  try {
    new Request('/api');
    return '';
  } catch {
    return typeof location === 'undefined' ? '' : location.origin;
  }
}

/**
 * `fetch`, looked up per call rather than bound once.
 *
 * `createClient` would otherwise capture `globalThis.fetch` as this module
 * loads, and a test that replaces the global afterwards would reach the real
 * daemon instead of its own mock.
 */
const lateBoundFetch = (request: Request): Promise<Response> => globalThis.fetch(request);

/**
 * Throttled so a burst of parallel 401s produces one prompt, not a storm.
 *
 * Still a `window` event rather than a `bus` one: `AuthTokenPrompt` and
 * `terminal-availability` both listen on `window`, and the bus migration of
 * those two listeners is not this task's.
 */
let lastAuthNotify = 0;
function notifyAuthRequired(): void {
  try {
    const now = Date.now();
    if (now - lastAuthNotify < 5000) return;
    lastAuthNotify = now;
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));
  } catch {
    // non-browser context
  }
}

/** Forgets the throttle, so one test's 401 does not silence the next one's. */
export function resetAuthThrottleForTests(): void {
  lastAuthNotify = 0;
}

/**
 * The 401 re-prompt, on every call rather than on the ones that remember it.
 *
 * It sits in middleware because it must happen whether or not the caller
 * unwraps the answer: a listing that swallows its own failure still leaves a
 * user signed out, and with no prompt they have no way back in.
 */
const reprompt: Middleware = {
  onResponse({ response }) {
    if (response.status === 401) notifyAuthRequired();
  },
};

export const client = createClient<paths>({
  baseUrl: apiBaseUrl(),
  fetch: lateBoundFetch,
  credentials: 'same-origin',
  headers: { [PLUGIN_CALLER_HEADER]: APP_CALLER },
});

client.use(reprompt);

/**
 * The human half of a `WebError`, which every crucible-web route serializes as
 * `{"error": {code, message}}`.
 *
 * Throwing the raw body instead puts a JSON blob in a toast: the user reads
 * `{"error":{"code":422,"message":"Hunk no longer exists"}}` where the server
 * went to the trouble of writing a sentence. A body that is not an envelope —
 * a plain-text 500 from a proxy, an empty body — reads back unchanged.
 *
 * `openapi-fetch` parses the failed body before this sees it, so the argument
 * is the parsed value for JSON and the raw text for everything else.
 */
export function errorSentence(error: unknown): string {
  if (error === undefined || error === null) return '';
  if (typeof error === 'string') return error;
  const message = (error as { error?: { message?: unknown } }).error?.message;
  if (typeof message === 'string' && message) return message;
  return JSON.stringify(error);
}

/**
 * One answer of `openapi-fetch`, as much of it as `decode` reads.
 *
 * Written here rather than imported because `FetchResponse` needs the
 * operation type to name itself, and `decode` is deliberately generic over
 * every route.
 */
type ApiResult<T> =
  | { data: T; error?: never; response: Response }
  | { data?: never; error: unknown; response: Response };

/** Builds the error a refused call throws, and raises the toast if one is due. */
function failure(
  response: Response,
  error: unknown,
  errorMessage: string,
  options: FailureOptions,
): ApiError {
  const sentence = errorSentence(error);
  const hint =
    response.status === 401
      ? ' — Unauthorized: sign in with the API key (from `cru web key` on the host)'
      : '';
  const message = sentence
    ? `${errorMessage}: ${sentence}`
    : `${errorMessage}: HTTP ${response.status}`;
  // A 401 already raised the sign-in prompt, so a toast beside it says the
  // same thing twice.
  if (options.notify && response.status !== 401) {
    notificationActions.addNotification('error', message + hint);
  }
  return Object.assign(new Error(message + hint), { status: response.status }) as ApiError;
}

/**
 * Throws the daemon's reason when the call failed, and answers nothing.
 *
 * For the writes whose reply no caller reads. `errorMessage` says what was
 * being attempted, which is the half the server cannot supply: "Failed to list
 * folder: root is not a registered project…" names both what broke and why,
 * where either half alone leaves the user guessing.
 */
export function expectOk(
  result: ApiResult<unknown>,
  errorMessage: string,
  options: FailureOptions = {},
): void {
  if (result.response.ok) return;
  throw failure(result.response, result.error, errorMessage, options);
}

/**
 * The reply body, or the daemon's reason for refusing.
 *
 * A reply that parsed to nothing is a failure here rather than an `undefined`
 * the caller reads a field off. `openapi-fetch` answers `undefined` for an
 * empty body and for a 204, so a route that answered neither the shape nor an
 * error status used to reach a component as `Cannot read properties of
 * undefined`. Use {@link expectOk} for the calls that expect no body.
 */
export function decode<T>(
  result: ApiResult<T>,
  errorMessage: string,
  options: FailureOptions = {},
): NonNullable<T> {
  expectOk(result, errorMessage, options);
  if (result.data === undefined || result.data === null) {
    throw Object.assign(new Error(`${errorMessage}: the server's reply carried no body`), {
      status: result.response.status,
    }) as ApiError;
  }
  return result.data as NonNullable<T>;
}

/**
 * Who one call says it is, as the six plugin routes declare the parameter.
 *
 * Those routes carry `x-crucible-plugin` in the document as a REQUIRED header
 * parameter, so the contract already refuses a call that names nobody — the
 * check the server makes is now also a check the compiler makes. The name is
 * spelled the way the document spells it, which HTTP treats as the same
 * header as {@link PLUGIN_CALLER_HEADER}.
 *
 * The client names the app on every request. A block drawing a plugin's own
 * command names the plugin instead, and `openapi-fetch` merges the parameter
 * after the client's default, so the plugin wins.
 */
export function callerParam(caller: string = APP_CALLER): { 'x-crucible-plugin': string } {
  return { 'x-crucible-plugin': caller };
}
