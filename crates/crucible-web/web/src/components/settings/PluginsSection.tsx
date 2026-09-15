// src/components/settings/PluginsSection.tsx
//
// The installed plugins, each with its state and a reload button, over the
// install rows.
import { Component, Show, For, createSignal, onMount } from 'solid-js';
import { Package } from '@/lib/icons';

import { SettingsSectionState } from './primitives';
import { PluginInstallRows } from './PluginInstall';
import type { PluginInfo } from '@/lib/api';
import { pluginVersionLabel } from '@/lib/plugin-version';
import { getPlugins, reloadPlugin } from '@/lib/api';

export const PluginsSection: Component<{ onChanged?: () => void | Promise<unknown> }> = (props) => {
  const [plugins, setPlugins] = createSignal<PluginInfo[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [reloadingPlugin, setReloadingPlugin] = createSignal<string | null>(null);

  const loadPlugins = async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await getPlugins();
      setPlugins(list);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load plugins');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadPlugins);

  const handleReload = async (name: string) => {
    setReloadingPlugin(name);
    try {
      await reloadPlugin(name);
      // Refresh list after reload
      await loadPlugins();
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to reload ${name}`);
    } finally {
      setReloadingPlugin(null);
    }
  };

  return (
    <SettingsSectionState
      title="Plugins"
      icon={Package}
      loading={loading()}
      error={error()}
      loadingMessage="Loading plugins…"
      isEmpty={false}
    >
      <PluginInstallRows
        onInstalled={async () => {
          await loadPlugins();
          // The declared trees too, so a plugin that ships settings gets its
          // pane in the left list without a restart.
          await props.onChanged?.();
        }}
      />
      <Show when={plugins().length === 0}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-sm text-muted-dark">
            No plugins discovered.
          </td>
        </tr>
      </Show>
      <For each={plugins()}>
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
