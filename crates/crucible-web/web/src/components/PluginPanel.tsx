import { Component, Show, For, createResource, createSignal } from 'solid-js';
import { getPlugins, reloadPlugin, installPlugin, removePlugin } from '@/lib/api';
import type { PluginInfo } from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';

/**
 * Quick client-side check on URLs the user types into the install modal.
 * The daemon validates more strictly (clone has to succeed), but we
 * reject obvious mis-types here so the user doesn't wait 10 seconds for
 * a clone to fail with a confusing error.
 */
const VALID_URL_PREFIXES = ['github:', 'git+', 'path:', 'https://', 'http://', 'git@'];
function looksLikeValidUrl(url: string): boolean {
  const trimmed = url.trim();
  if (trimmed.length === 0) return false;
  if (VALID_URL_PREFIXES.some((p) => trimmed.startsWith(p))) return true;
  // Allow "user/repo" shorthand (github default).
  if (/^[a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+$/.test(trimmed)) return true;
  return false;
}

function sourceColor(source: string): string {
  switch (source) {
    case 'User':
      return 'bg-blue-900/40 text-blue-300 border-blue-700/50';
    case 'Runtime':
      return 'bg-emerald-900/40 text-emerald-300 border-emerald-700/50';
    case 'EnvPath':
      return 'bg-purple-900/40 text-purple-300 border-purple-700/50';
    case 'Builtin':
      return 'bg-neutral-700/40 text-neutral-300 border-neutral-600/50';
    default:
      return 'bg-neutral-800 text-neutral-400 border-neutral-700';
  }
}

function stateColor(state: string): string {
  switch (state) {
    case 'Active':
      return 'bg-emerald-900/40 text-emerald-300 border-emerald-700/50';
    case 'Error':
      return 'bg-red-900/40 text-red-300 border-red-700/50';
    case 'Disabled':
      return 'bg-neutral-800 text-neutral-500 border-neutral-700';
    default:
      return 'bg-neutral-800 text-neutral-400 border-neutral-700';
  }
}

export const PluginPanel: Component = () => {
  const [plugins, { refetch }] = createResource<PluginInfo[]>(async () => {
    try {
      return await getPlugins();
    } catch (err) {
      notificationActions.addNotification('error', `Failed to load plugins: ${err}`);
      return [];
    }
  });

  const [reloading, setReloading] = createSignal<string | null>(null);
  const [removing, setRemoving] = createSignal<string | null>(null);
  const [showInstall, setShowInstall] = createSignal(false);
  const [installUrl, setInstallUrl] = createSignal('');
  const [installing, setInstalling] = createSignal(false);
  const [confirmRemove, setConfirmRemove] = createSignal<{ name: string; purge: boolean } | null>(null);

  const handleInstall = async () => {
    const url = installUrl().trim();
    if (!looksLikeValidUrl(url)) {
      notificationActions.addNotification(
        'error',
        'Invalid URL. Use "user/repo", "github:user/repo", "https://...", or "git@...".',
      );
      return;
    }
    setInstalling(true);
    try {
      const result = await installPlugin({ url });
      const status =
        result.outcome.kind === 'cloned'
          ? `Installed ${result.name}`
          : `${result.name} already present; declared in plugins.toml`;
      notificationActions.addNotification('success', status);
      setInstallUrl('');
      setShowInstall(false);
      await refetch();
    } catch (err) {
      notificationActions.addNotification('error', `Install failed: ${err}`);
    } finally {
      setInstalling(false);
    }
  };

  const handleRemoveConfirmed = async () => {
    const target = confirmRemove();
    if (!target) return;
    setRemoving(target.name);
    setConfirmRemove(null);
    try {
      const result = await removePlugin(target.name, target.purge);
      const dirNote = result.purged_dir ? ` (deleted ${result.purged_dir})` : '';
      notificationActions.addNotification('success', `Removed ${target.name}${dirNote}`);
      await refetch();
    } catch (err) {
      notificationActions.addNotification('error', `Remove failed: ${err}`);
    } finally {
      setRemoving(null);
    }
  };

  const handleReload = async (name: string) => {
    setReloading(name);
    try {
      const result = await reloadPlugin(name);
      notificationActions.addNotification(
        'success',
        `Reloaded ${name}: ${result.tools}T ${result.commands}C ${result.handlers}H ${result.services}S`,
      );
      await refetch();
    } catch (err) {
      notificationActions.addNotification('error', `Failed to reload ${name}: ${err}`);
    } finally {
      setReloading(null);
    }
  };

  return (
    <PanelShell class="relative">
      <PanelHeader title="Plugins">
        <div class="mt-1 flex items-center gap-2">
          <button
            type="button"
            onClick={() => setShowInstall(true)}
            class="text-[11px] px-2 py-0.5 rounded bg-blue-700 hover:bg-blue-600 text-white"
            data-testid="plugins-install-open"
          >
            Install
          </button>
          <button
            type="button"
            onClick={() => refetch()}
            class="text-[11px] text-neutral-400 hover:text-neutral-200"
            data-testid="plugins-refresh"
          >
            Refresh
          </button>
        </div>
      </PanelHeader>

      <div class="flex-1 overflow-y-auto">
        <Show
          when={!plugins.loading}
          fallback={<div class="p-4 text-sm text-neutral-500">Loading…</div>}
        >
          <Show
            when={(plugins() ?? []).length > 0}
            fallback={
              <div class="p-4 text-sm text-neutral-500">
                No plugins discovered. Try <code>cru install &lt;repo&gt;</code>.
              </div>
            }
          >
            <For each={plugins()}>
              {(plugin) => (
                <div
                  class="px-3 py-2 border-b border-neutral-850 hover:bg-neutral-800/30"
                  data-testid={`plugin-row-${plugin.name}`}
                >
                  <div class="flex items-center gap-2">
                    <span class="flex-1 text-sm font-mono text-neutral-200 truncate">
                      {plugin.name}
                    </span>
                    <span class="text-xs text-neutral-500">v{plugin.version}</span>
                    <button
                      type="button"
                      class="text-xs px-2 py-0.5 bg-neutral-800 hover:bg-neutral-700 rounded border border-neutral-700 text-neutral-200 disabled:opacity-50 disabled:cursor-not-allowed"
                      onClick={() => handleReload(plugin.name)}
                      disabled={reloading() === plugin.name}
                      data-testid={`plugin-reload-${plugin.name}`}
                    >
                      {reloading() === plugin.name ? '↻' : 'Reload'}
                    </button>
                    <button
                      type="button"
                      class="text-xs px-2 py-0.5 bg-red-900/40 hover:bg-red-800/60 rounded border border-red-800/60 text-red-200 disabled:opacity-50 disabled:cursor-not-allowed"
                      onClick={() => setConfirmRemove({ name: plugin.name, purge: false })}
                      disabled={removing() === plugin.name}
                      data-testid={`plugin-remove-${plugin.name}`}
                    >
                      {removing() === plugin.name ? '…' : 'Uninstall'}
                    </button>
                  </div>
                  <div class="mt-1 flex items-center gap-1.5">
                    <span
                      class={`text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded border ${sourceColor(plugin.source)}`}
                    >
                      {plugin.source}
                    </span>
                    <span
                      class={`text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded border ${stateColor(plugin.state)}`}
                    >
                      {plugin.state}
                    </span>
                    <span class="text-[11px] text-neutral-500" title="Tools · Commands · Handlers · Services">
                      {plugin.tools}T {plugin.commands}C {plugin.handlers}H {plugin.services}S
                    </span>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </Show>
      </div>

      {/* Install modal */}
      <Show when={showInstall()}>
        <div
          class="absolute inset-0 bg-neutral-900/95 z-10 flex flex-col p-4"
          data-testid="plugins-install-modal"
        >
          <h3 class="text-sm font-semibold text-neutral-200 mb-2">Install plugin</h3>
          <p class="text-xs text-neutral-500 mb-2">
            Accepts <code>user/repo</code>, <code>github:user/repo</code>,
            <code> https://…</code>, or <code>git@…</code>.
          </p>
          <input
            type="text"
            value={installUrl()}
            onInput={(e) => setInstallUrl(e.currentTarget.value)}
            placeholder="user/repo or https://…"
            disabled={installing()}
            class="w-full bg-neutral-800 text-neutral-100 text-sm rounded px-2 py-1.5 border border-neutral-700 focus:outline-none focus:border-neutral-500 disabled:opacity-50"
            data-testid="plugins-install-url"
          />
          <div class="mt-3 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={() => {
                setShowInstall(false);
                setInstallUrl('');
              }}
              disabled={installing()}
              class="text-xs px-3 py-1 text-neutral-400 hover:text-neutral-200 disabled:opacity-50"
              data-testid="plugins-install-cancel"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleInstall}
              disabled={installing()}
              class="text-xs px-3 py-1 bg-blue-700 hover:bg-blue-600 rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
              data-testid="plugins-install-submit"
            >
              {installing() ? 'Installing…' : 'Install'}
            </button>
          </div>
        </div>
      </Show>

      {/* Remove confirmation */}
      <Show when={confirmRemove()}>
        {(target) => (
          <div
            class="absolute inset-0 bg-neutral-900/95 z-10 flex flex-col p-4"
            data-testid="plugins-remove-modal"
          >
            <h3 class="text-sm font-semibold text-neutral-200 mb-2">
              Uninstall {target().name}?
            </h3>
            <p class="text-xs text-neutral-500 mb-3">
              This removes the entry from <code>plugins.toml</code>. Optionally also
              delete the cloned plugin directory.
            </p>
            <label class="flex items-center gap-2 text-xs text-neutral-300 mb-3">
              <input
                type="checkbox"
                checked={target().purge}
                onChange={(e) => setConfirmRemove({ ...target(), purge: e.currentTarget.checked })}
                data-testid="plugins-remove-purge"
              />
              Also delete plugin directory
            </label>
            <div class="flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmRemove(null)}
                class="text-xs px-3 py-1 text-neutral-400 hover:text-neutral-200"
                data-testid="plugins-remove-cancel"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleRemoveConfirmed}
                class="text-xs px-3 py-1 bg-red-700 hover:bg-red-600 rounded text-white"
                data-testid="plugins-remove-confirm"
              >
                Uninstall
              </button>
            </div>
          </div>
        )}
      </Show>
    </PanelShell>
  );
};
