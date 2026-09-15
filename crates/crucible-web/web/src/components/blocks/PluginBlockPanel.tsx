import { Component, For, Show, createResource, createSignal } from 'solid-js';
import { PanelShell } from '../PanelShell';
import { PanelHeader } from '../PanelHeader';
import { PluginBlock } from './PluginBlock';
import { PluginCommandDialog } from './PluginCommandDialog';
import { getPluginCommands, getPluginPublications, type PluginCommand } from '@/lib/api';

/**
 * A plugin block as a dockable panel, rather than as a block inside a note.
 *
 * Until this existed a block could mount in exactly one place —
 * `MarkdownPreview`, through the ```plugin fence — so a plugin could contribute
 * content to a document and could not contribute a panel. That blocked every
 * candidate for rebuilding an existing panel as a plugin, because none of them
 * live in a note.
 *
 * The block itself is unchanged: `PluginBlock` resolves a registered component
 * or falls back to the generic renderer, exactly as it does in a document. What
 * differs is only the frame around it and where the parameters come from — a
 * fence carries its own, a panel has none, so a panel-hosted block gets an
 * empty table and must read what it needs from its publication.
 */

/** Everything published, as `plugin/key` pairs a panel can offer. */
async function publishedBlocks(): Promise<Array<{ plugin: string; key: string }>> {
  const all = await getPluginPublications();
  const out: Array<{ plugin: string; key: string }> = [];
  for (const [key, byPlugin] of Object.entries(all)) {
    for (const plugin of Object.keys(byPlugin as Record<string, unknown>)) {
      out.push({ plugin, key });
    }
  }
  out.sort((a, b) => `${a.plugin}/${a.key}`.localeCompare(`${b.plugin}/${b.key}`));
  return out;
}

/**
 * Every declared command, grouped by plugin through the sort.
 *
 * `getPluginCommands` had no caller at all until this one: the daemon has
 * always known what a plugin can be asked to do, and nothing carried it to a
 * place a person could press. This is that place — the panel host that already
 * exists, rather than a new surface, because the *placement* is the expensive
 * half of offering a primitive as a button and this one is already paid for.
 */
async function offeredCommands(): Promise<PluginCommand[]> {
  const commands = await getPluginCommands();
  return [...commands].sort((a, b) =>
    `${a.plugin}/${a.name}`.localeCompare(`${b.plugin}/${b.name}`),
  );
}

/**
 * A publication key is `<plugin>:<block>` by convention (kanban publishes
 * `kanban:board`). Split on the first colon so a panel can address the block;
 * a key with no colon addresses a block of the same name.
 */
function blockNameOf(key: string): string {
  const colon = key.indexOf(':');
  return colon >= 0 ? key.slice(colon + 1) : key;
}

export const PluginBlockPanel: Component = () => {
  const [selected, setSelected] = createSignal<{ plugin: string; key: string } | null>(null);
  const [available] = createResource(publishedBlocks);
  const [commands] = createResource(offeredCommands);
  const [running, setRunning] = createSignal<PluginCommand | null>(null);

  return (
    <PanelShell class="overflow-hidden">
      <PanelHeader title="Plugin Blocks" />
      <div class="flex-1 overflow-y-auto p-3">
        <Show
          when={selected()}
          fallback={
            <Show
              when={(available() ?? []).length > 0}
              fallback={
                <div class="text-sm text-muted italic">
                  No plugin has published anything yet. A plugin appears here as soon as it calls{' '}
                  <code>cru.plugin.publish</code>.
                </div>
              }
            >
              <div class="flex flex-col gap-1">
                <For each={available() ?? []}>
                  {(entry) => (
                    <button
                      type="button"
                      class="text-left rounded border border-hairline px-2 py-1.5 text-sm
                             hover:border-primary transition-colors"
                      onClick={() => setSelected(entry)}
                    >
                      <span class="font-medium">{entry.plugin}</span>
                      <span class="text-muted"> / {blockNameOf(entry.key)}</span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          }
        >
          {(entry) => (
            <div>
              <button
                type="button"
                class="mb-2 text-xs text-muted hover:text-shell-ink"
                onClick={() => setSelected(null)}
              >
                ← all blocks
              </button>
              <PluginBlock
                plugin={entry().plugin}
                block={blockNameOf(entry().key)}
                params={{}}
              />
            </div>
          )}
        </Show>

        <Show when={!selected() && (commands() ?? []).length > 0}>
          <div class="mt-4 border-t border-hairline pt-3">
            <div class="mb-2 text-xs uppercase tracking-wide text-muted">Commands</div>
            <div class="flex flex-col gap-1">
              <For each={commands() ?? []}>
                {(command) => (
                  <button
                    type="button"
                    class="flex items-center gap-2 text-left rounded border border-hairline
                           px-2 py-1.5 text-sm hover:border-primary transition-colors"
                    onClick={() => setRunning(command)}
                  >
                    <span class="min-w-0 flex-1 truncate">
                      <span class="text-muted">{command.plugin} / </span>
                      <span class="font-mono">{command.name}</span>
                    </span>
                    {/* Declared by the plugin, verified by nothing — the title
                        says so, because a badge that reads as a guarantee is
                        the failure mode the plan names. */}
                    <span
                      class={`shrink-0 rounded border px-1.5 py-0.5 text-floor uppercase ${
                        command.effect === 'read'
                          ? 'text-muted border-hairline'
                          : 'text-attention border-attention/50'
                      }`}
                      title="The plugin declares this about itself. Nothing verifies it."
                    >
                      {command.effect ?? 'write'}
                    </span>
                  </button>
                )}
              </For>
            </div>
          </div>
        </Show>
      </div>
      <PluginCommandDialog command={running()} onClose={() => setRunning(null)} />
    </PanelShell>
  );
};
