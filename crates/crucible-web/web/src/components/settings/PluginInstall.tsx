import { Component, Show, createSignal } from 'solid-js';
import { Package } from '@/lib/icons';
import { useInstallPlugin } from '@/lib/query/plugins';
import { notificationActions } from '@/stores/notificationStore';

/**
 * Install a plugin from a git URL, from inside settings.
 *
 * The RPC and the route already existed (`POST /api/plugins`, which takes
 * `{url, branch, pin}` and normalises the URL against its own allowlist); what
 * was missing was any way to reach them without a terminal. The field takes a
 * full git URL or the `user/repo` shorthand the daemon expands, and the daemon
 * remains the only judge of which URLs are admissible — nothing here re-states
 * that rule.
 *
 * **One confirmation, naming the URL.** That is not new policy: a plugin runs
 * with the user's own reach, capabilities are not enforceable, and user
 * installation IS the security boundary this project relies on. So the boundary
 * is drawn where the user can see it — one step, the URL spelled out, before
 * anything is cloned. A second dialog after that would train the user to click
 * through both.
 *
 * The install mutation invalidates the roster, the declared trees and the
 * commands, so a plugin that declares `cru.plugin.options{}` gets its settings
 * pane immediately, in every list that draws one. This used to be a callback
 * the caller had to remember to pass, and the panel's own install forgot.
 */
export const PluginInstallRows: Component = () => {
  const installMutation = useInstallPlugin();
  const [url, setUrl] = createSignal('');
  const [confirming, setConfirming] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const ask = () => {
    const target = url().trim();
    setError(null);
    if (!target) return;
    setConfirming(target);
  };

  const install = async () => {
    const target = confirming();
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      const result = await installMutation.mutateAsync({ url: target });
      setConfirming(null);
      setUrl('');
      if (!result.loaded) {
        // Installed but not running is not success: the settings pane the user
        // came for will not appear, and the reason is the daemon's to state.
        setError(result.error ?? `${result.name} installed but did not load`);
      } else {
        notificationActions.addNotification('success', `Installed ${result.name}`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <tr class="border-b border-hairline align-top">
        <td class="py-3 pr-4">
          <div class="text-sm text-shell-body">Install a plugin</div>
          <p class="mt-0.5 max-w-[34rem] text-floor leading-4 text-muted-dark">
            Paste a git URL, or the <code>user/repo</code> shorthand for GitHub.
          </p>
        </td>
        <td class="py-3 text-right">
          <div class="flex items-center justify-end gap-2">
            <input
              type="text"
              data-testid="plugin-install-url"
              placeholder="user/repo"
              class="w-56 max-w-full rounded border border-hairline bg-control px-2 py-1 text-sm
                     text-shell-ink focus:border-primary focus-ring disabled:opacity-50"
              value={url()}
              disabled={busy()}
              onInput={(e) => setUrl(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') ask();
              }}
            />
            <button
              type="button"
              data-testid="plugin-install-submit"
              class="focus-ring rounded bg-control px-2 py-1 text-xs text-shell-body
                     transition-colors hover:bg-hover-wash disabled:opacity-50"
              disabled={busy() || !url().trim()}
              onClick={ask}
            >
              Install
            </button>
          </div>
        </td>
      </tr>

      {/* The one confirmation, and it names the URL it will clone from. */}
      <Show when={confirming()}>
        {(target) => (
          <tr class="border-b border-hairline">
            <td colSpan={2} class="py-3">
              <div
                class="rounded border border-hairline-strong bg-surface-elevated p-3"
                data-testid="plugin-install-confirm"
              >
                <div class="flex items-start gap-2">
                  <Package class="mt-0.5 h-4 w-4 flex-none text-primary" />
                  <div class="text-sm text-shell-body">
                    Clone and run <span class="font-semibold">{target()}</span>?
                    <p class="mt-1 text-floor leading-4 text-muted-dark">
                      A plugin runs with your own reach: it can read and write your files and start
                      commands. Install a plugin only from a source you trust.
                    </p>
                  </div>
                </div>
                <div class="mt-3 flex justify-end gap-2">
                  <button
                    type="button"
                    data-testid="plugin-install-cancel"
                    class="focus-ring rounded px-2 py-1 text-xs text-muted hover:text-shell-ink"
                    disabled={busy()}
                    onClick={() => setConfirming(null)}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    data-testid="plugin-install-confirm-submit"
                    class="focus-ring rounded bg-control px-2 py-1 text-xs text-shell-ink
                           hover:bg-hover-wash disabled:opacity-50"
                    disabled={busy()}
                    onClick={() => void install()}
                  >
                    {busy() ? 'Installing…' : 'Install'}
                  </button>
                </div>
              </div>
            </td>
          </tr>
        )}
      </Show>

      <Show when={error()}>
        <tr>
          <td colSpan={2} class="py-2 text-center text-xs text-error" data-testid="plugin-install-error">
            {error()}
          </td>
        </tr>
      </Show>
    </>
  );
};
