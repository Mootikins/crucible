import { Component, Show, createSignal, onCleanup } from 'solid-js';
import { getApiToken, setApiToken } from '@/lib/api';

interface AuthTokenPromptProps {
  /** Called after the token is saved. Defaults to a full reload so every
   * context refetches with credentials. Injectable for tests. */
  onSaved?: () => void;
}

/**
 * Modal that appears when the server rejects API calls with 401
 * (`crucible:auth-required`, dispatched by the api layer). Lets a user on a
 * remote device paste the API key without crafting a `?token=` URL. The key
 * lives in `~/.config/crucible/api_key` on the machine running `cru web`.
 */
export const AuthTokenPrompt: Component<AuthTokenPromptProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [value, setValue] = createSignal('');

  const onAuthRequired = () => setOpen(true);
  window.addEventListener('crucible:auth-required', onAuthRequired);
  onCleanup(() => window.removeEventListener('crucible:auth-required', onAuthRequired));

  const save = () => {
    const token = value().trim();
    if (!token) return;
    setApiToken(token);
    setOpen(false);
    (props.onSaved ?? (() => window.location.reload()))();
  };

  // NOTE: the <Show> must not be the component's root — a root-position
  // conditional fails to re-render under the vitest/solid setup (verified
  // empirically; nested conditionals behave). The wrapper div is inert.
  return (
    <div>
      <Show when={open()}>
      <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" data-testid="auth-token-prompt">
        <div class="w-full max-w-md rounded-lg border border-neutral-700 bg-neutral-900 p-5 shadow-xl">
          <h2 class="mb-1 text-base font-semibold text-neutral-100">API token required</h2>
          <p class="mb-3 text-sm text-neutral-400">
            This server requires a token for non-localhost access
            {getApiToken() ? ' — the stored token was rejected.' : '.'} Paste the key from{' '}
            <code class="text-neutral-300">~/.config/crucible/api_key</code> on the host machine.
          </p>
          <input
            type="password"
            value={value()}
            onInput={(e) => setValue(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') save();
            }}
            placeholder="API token"
            class="mb-3 w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-neutral-100 outline-none focus:border-blue-500"
            data-testid="auth-token-input"
          />
          <div class="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setOpen(false)}
              class="rounded px-3 py-1.5 text-sm text-neutral-400 hover:text-neutral-200"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={save}
              disabled={!value().trim()}
              class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
              data-testid="auth-token-save"
            >
              Save & reload
            </button>
          </div>
        </div>
      </div>
    </Show>
    </div>
  );
};
