import { Component, Show, For, createResource, createSignal } from 'solid-js';
import { getPlugins, reloadPlugin } from '@/lib/api';
import type { PluginInfo } from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';

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
    <PanelShell>
      <PanelHeader title="Plugins">
        <button
          type="button"
          onClick={() => refetch()}
          class="ml-2 text-[11px] text-neutral-400 hover:text-neutral-200"
          data-testid="plugins-refresh"
        >
          Refresh
        </button>
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
    </PanelShell>
  );
};
