// src/components/settings/PluginsSection.tsx
//
// The installed plugins, each with its state and a reload button, over the
// install rows.
import { Component, Show, For, createSignal } from 'solid-js';
import { Package } from '@/lib/icons';

import { SettingsSectionState } from './primitives';
import { PluginInstallRows } from './PluginInstall';
import { pluginVersionLabel } from '@/lib/plugin-version';
import { usePluginList, useReloadPlugin } from '@/lib/query/plugins';

/**
 * This section used to hold its own copy of the roster, filled once on mount
 * and refreshed only by its own reload button. `PluginPanel` held another one,
 * so a reload here left the panel describing the plugin as it was before.
 * Both now read one query, and the reload mutation invalidates it.
 */
export const PluginsSection: Component = () => {
  const plugins = usePluginList();
  const reloadMutation = useReloadPlugin();
  const [reloadingPlugin, setReloadingPlugin] = createSignal<string | null>(null);
  const [reloadError, setReloadError] = createSignal<string | null>(null);

  const error = () => reloadError() ?? plugins.error?.message ?? null;

  const handleReload = async (name: string) => {
    setReloadingPlugin(name);
    setReloadError(null);
    try {
      await reloadMutation.mutateAsync(name);
    } catch (err) {
      setReloadError(err instanceof Error ? err.message : `Failed to reload ${name}`);
    } finally {
      setReloadingPlugin(null);
    }
  };

  return (
    <SettingsSectionState
      title="Plugins"
      icon={Package}
      loading={plugins.isPending}
      error={error()}
      loadingMessage="Loading plugins…"
      isEmpty={false}
    >
      {/* No callback: the install mutation invalidates the roster and the
          declared trees, so this list and the left one refresh themselves. */}
      <PluginInstallRows />
      <Show when={(plugins.data ?? []).length === 0}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-sm text-muted-dark">
            No plugins discovered.
          </td>
        </tr>
      </Show>
      <For each={plugins.data}>
        {(plugin) => (
          <tr class="border-b border-hairline">
            <td class="py-2.5 text-shell-body text-sm">
              <div class="flex items-center gap-2">
                <span
                  class={`inline-block w-2 h-2 rounded-full ${
                    plugin.state === 'Active' ? 'bg-ok' : 'bg-error'
                  }`}
                  title={`State: ${plugin.state}`}
                />
                <div>
                  <div class="text-sm">{plugin.name} <span class="text-xs text-muted-dark" data-testid={`plugin-version-${plugin.name}`}>{pluginVersionLabel(plugin.version)}</span></div>
                  <div class="text-xs text-muted-dark">{plugin.source} · {plugin.tools}T {plugin.commands}C {plugin.handlers}H {plugin.services}S</div>
                </div>
              </div>
            </td>
            <td class="py-2.5 text-right">
              <button
                onClick={() => handleReload(plugin.name)}
                disabled={reloadingPlugin() === plugin.name}
                class="px-2 py-1 text-xs rounded bg-control hover:bg-hover-wash text-shell-body transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {reloadingPlugin() === plugin.name ? '↻' : 'Reload'}
              </button>
            </td>
          </tr>
        )}
      </For>
    </SettingsSectionState>
  );
};
