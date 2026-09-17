import { Component, Show, createSignal } from 'solid-js';
import { useLogin } from '@/lib/query/auth';
import { getBus } from '@/lib/bus';

interface AuthTokenPromptProps {
  /** Called after a successful sign-in. Defaults to a full reload so every
   * context refetches with credentials. Injectable for tests. */
  onSaved?: () => void;
}

/**
 * Modal that appears when the server rejects API calls with 401 (`authRequired`
 * on the bus, emitted by the api layer). The pasted key is
 * exchanged for an HttpOnly session cookie via POST /api/auth/login — it is
 * never stored where page JS can read it and never travels in a URL. The key
 * lives in `~/.config/crucible/api_key` on the machine running `cru web`
 * (`cru web key` prints it).
 */
export const AuthTokenPrompt: Component<AuthTokenPromptProps> = (props) => {
  const loginMutation = useLogin();
  const [open, setOpen] = createSignal(false);
  const [value, setValue] = createSignal('');
  const [rejected, setRejected] = createSignal(false);

  // `on` binds the removal to this component's owner, so there is no
  // `onCleanup` here.
  getBus().on('authRequired', () => setOpen(true));

  const save = async () => {
    const key = value().trim();
    if (await loginMutation.mutateAsync(key)) {
      setOpen(false);
      (props.onSaved ?? (() => window.location.reload()))();
    } else {
      setRejected(true);
    }
  };

  // NOTE: the <Show> must not be the component's root — a root-position
  // conditional fails to re-render under the vitest/solid setup (verified
  // empirically; nested conditionals behave). The wrapper div is inert.
  return (
    <div>
      <Show when={open()}>
      <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" data-testid="auth-token-prompt">
        <div class="w-full max-w-md rounded-lg border border-hairline bg-surface-overlay p-5 shadow-xl">
          <h2 class="mb-1 text-base font-semibold text-shell-ink">Sign in</h2>
          <p class="mb-3 text-sm text-muted">
            This server requires an API key for non-localhost access. Paste the key from{' '}
            <code class="text-shell-body">cru web key</code> on the host machine.
          </p>
          <Show when={rejected()}>
            <p class="mb-3 text-sm text-error" data-testid="auth-token-rejected">
              The server rejected that key.
            </p>
          </Show>
          <input
            type="password"
            value={value()}
            onInput={(e) => setValue(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void save();
            }}
            placeholder="API key"
            class="mb-3 w-full rounded border border-hairline bg-control px-3 py-2 text-sm text-shell-ink focus-ring focus:border-primary"
            data-testid="auth-token-input"
          />
          <div class="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setOpen(false)}
              class="rounded px-3 py-1.5 text-sm text-muted hover:text-shell-ink"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => void save()}
              disabled={!value().trim()}
              class="rounded bg-primary px-3 py-1.5 text-sm font-medium text-on-primary hover:bg-primary-hover disabled:opacity-50"
              data-testid="auth-token-save"
            >
              Sign in & reload
            </button>
          </div>
        </div>
      </div>
    </Show>
    </div>
  );
};
