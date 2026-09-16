import { Component, For, Index, Show, createSignal } from 'solid-js';
import type { PluginOptionNode } from '@/lib/api';
import {
  useExecutePluginOption,
  usePluginOption,
  useSetPluginOption,
} from '@/lib/query/plugins';
import { notificationActions } from '@/stores/notificationStore';

/**
 * Renders a plugin's settings tree — the web half of `crucible.options`.
 *
 * Knows nothing about any particular plugin or option. A node arrives as
 * `{type, name, desc, …}` and this switches on `type` alone, so a plugin
 * shipped tomorrow gets a settings pane here for free. That is the whole
 * reason the tree is declared in Lua and projected rather than each frontend
 * being handed a bespoke API: the same declaration feeds this and the TUI.
 *
 * An unrecognised `type` renders as a read-only value rather than vanishing.
 * A plugin using a widget kind added after this file was written should look
 * plain, not invisible — silence would leave the user unable to see a setting
 * that is nonetheless in effect.
 */

// Sized to the control column, not to the row: these now live in a settings
// table's right-hand cell beside every other section's controls.
const inputClass =
  'w-56 max-w-full px-2 py-1 rounded border border-hairline bg-control text-shell-ink ' +
  'text-sm focus-ring focus:border-primary disabled:opacity-50';

/**
 * A leaf's value, on the cache entry its plugin and path name.
 *
 * The row used to key a resource on the declaration object, so a reloaded tree
 * re-read every value with it. The entry is keyed on the plugin and the path
 * instead, and the write mutation invalidates every option of that plugin —
 * which is the same reconciliation, from the writer rather than from the
 * reload, so a second pane showing the same option is corrected too.
 */
const OptionRow: Component<{
  plugin: string;
  path: string[];
  node: PluginOptionNode;
  onChanged: () => void | Promise<unknown>;
}> = (props) => {
  const value = usePluginOption(
    () => props.plugin,
    () => props.path,
  );
  const setOption = useSetPluginOption(
    () => props.plugin,
    () => props.path,
  );
  const executeOption = useExecutePluginOption(
    () => props.plugin,
    () => props.path,
  );
  const [busy, setBusy] = createSignal(false);

  const editable = () => props.node.writable !== false && !props.node.disabled && !busy();

  const commit = async (next: unknown) => {
    setBusy(true);
    try {
      // The mutation paints `next` at once and puts the old value back if the
      // plugin refuses. On success it asks the plugin's options again, because
      // a setter is free to normalise or clamp what it stored.
      await setOption.mutateAsync(next);
      // Sibling DECLARATIONS can depend on this one (a `values` or `disabled`
      // function reading it), so the tree is re-read as well as the values.
      await props.onChanged();
    } catch (err) {
      notificationActions.addNotification('error', `${props.node.name ?? props.path.at(-1)}: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const press = async () => {
    setBusy(true);
    try {
      await executeOption.mutateAsync();
      await props.onChanged();
    } catch (err) {
      notificationActions.addNotification('error', `${props.node.name ?? 'Action'}: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  // A `<tr>`, not a `<div>`. Every settings section in the modal renders rows
  // into one shared table, so a plugin's pane sits on the same label column and
  // the same control column as Appearance or Model. Rendered as a block it read
  // as a foreign panel embedded in the settings dialog, which is exactly what
  // it was.
  return (
    <tr class="border-b border-hairline align-top" data-testid={`plugin-option-${props.path.join('-')}`}>
      <td class="py-3 pr-4">
        <Show when={props.node.type !== 'execute'}>
          <div class="text-sm text-shell-body">
            {props.node.name ?? props.path.at(-1)}
            <Show when={props.node.writable === false}>
              <span class="ml-1 text-floor text-muted" title="This setting is read-only">
                (read-only)
              </span>
            </Show>
          </div>
        </Show>
        <Show when={props.node.desc}>
          <p class="mt-0.5 max-w-[34rem] text-floor leading-4 text-muted-dark">{props.node.desc}</p>
        </Show>
      </td>
      <td class="py-3 text-right">
      <Show when={props.node.type === 'toggle'}>
        <input
          type="checkbox"
          class="align-middle"
          checked={value.data === true}
          disabled={!editable()}
          onChange={(e) => void commit(e.currentTarget.checked)}
        />
      </Show>

      <Show when={props.node.type === 'select'}>
        <select
          class={`cru-select ${inputClass}`}
          disabled={!editable()}
          value={String(value.data ?? '')}
          onChange={(e) => void commit(e.currentTarget.value)}
        >
          {/* An empty choice so an unset option can be left unset, and so a
              stored value the plugin no longer offers is visibly not one of
              the choices instead of silently reading as the first. */}
          <option value="">—</option>
          <For each={props.node.values ?? []}>
            {(choice) => <option value={String(choice.value)}>{choice.label}</option>}
          </For>
        </select>
      </Show>

      <Show when={props.node.type === 'range'}>
        <input
          type="number"
          class={inputClass}
          min={props.node.min}
          max={props.node.max}
          step={props.node.step}
          value={value.data === null || value.data === undefined ? '' : String(value.data)}
          disabled={!editable()}
          onChange={(e) => {
            const raw = e.currentTarget.value;
            void commit(raw === '' ? null : Number(raw));
          }}
        />
      </Show>

      <Show when={props.node.type === 'execute'}>
        <button
          type="button"
          class="px-2 py-1 text-xs rounded border border-hairline bg-surface-elevated
                 text-shell-body hover:border-primary disabled:opacity-50"
          disabled={busy() || props.node.disabled}
          onClick={() => void press()}
        >
          {props.node.name ?? props.path.at(-1)}
        </button>
      </Show>

      {/* `input` and anything this file does not recognise. An unknown widget
          kind renders as text rather than nothing — see the module comment. */}
      <Show when={!['toggle', 'select', 'range', 'execute'].includes(props.node.type)}>
        <input
          type="text"
          class={inputClass}
          value={value.data === null || value.data === undefined ? '' : String(value.data)}
          disabled={!editable()}
          onChange={(e) => {
            const raw = e.currentTarget.value;
            void commit(raw === '' ? null : raw);
          }}
        />
      </Show>

      </td>
    </tr>
  );
};

/**
 * A group and its children, recursively.
 *
 * `Index`, not `For`: a reloaded tree is a new object graph, so `For` would
 * discard and rebuild every row — losing focus mid-edit, and re-reading each
 * value on top of the read the row already did for itself. Keyed by position,
 * the rows survive and the new declaration reaches them as a prop.
 */
const OptionGroup: Component<{
  plugin: string;
  path: string[];
  node: PluginOptionNode;
  onChanged: () => void | Promise<unknown>;
}> = (props) => (
  <>
    {/* A nested group is a sub-heading in the same table — the same `<tr>` the
        app's own sections use, so a plugin's subsection and Crucible's own read
        identically. The ROOT group draws nothing: the left list already names
        the plugin. */}
    <Show when={props.path.length > 0 && props.node.name}>
      <tr>
        <td colSpan={2} class="pt-5 pb-2 text-floor font-semibold uppercase tracking-wider text-muted-dark">
          {props.node.name}
        </td>
      </tr>
    </Show>
    <Index each={props.node.args ?? []}>
      {(child) => {
        const path = () => [...props.path, child().key ?? ''];
        return (
          <Show when={!child().hidden}>
            <Show
              when={child().type === 'group'}
              fallback={
                <OptionRow
                  plugin={props.plugin}
                  path={path()}
                  node={child()}
                  onChanged={props.onChanged}
                />
              }
            >
              <OptionGroup
                plugin={props.plugin}
                path={path()}
                node={child()}
                onChanged={props.onChanged}
              />
            </Show>
          </Show>
        );
      }}
    </Index>
  </>
);

/**
 * A plugin's settings as table ROWS.
 *
 * The caller owns the `<table>`, because both callers already have one: the
 * settings modal renders every section into a single shared table, and
 * `PluginPanel` wraps its own. One markup, one layout, no drift.
 */
export const PluginSettings: Component<{
  plugin: string;
  tree: PluginOptionNode;
  /** Re-read the whole tree; resolves once the reloaded one is in hand. */
  onChanged: () => void | Promise<unknown>;
}> = (props) => (
  <Show
    when={(props.tree.args ?? []).length > 0}
    fallback={
      <tr>
        <td colSpan={2} class="py-3 text-sm text-muted">
          This plugin declares no settings.
        </td>
      </tr>
    }
  >
    <OptionGroup plugin={props.plugin} path={[]} node={props.tree} onChanged={props.onChanged} />
  </Show>
);
