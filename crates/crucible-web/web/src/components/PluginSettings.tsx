import { Component, For, Index, Show, createResource, createSignal } from 'solid-js';
import {
  getPluginOption,
  setPluginOption,
  executePluginOption,
  type PluginOptionNode,
} from '@/lib/api';
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

const inputClass =
  'w-full px-2 py-1 rounded border border-hairline bg-surface text-shell-body ' +
  'text-xs focus:outline-none focus:border-primary disabled:opacity-50';

/**
 * A leaf's value, fetched on mount and re-fetched when the tree reloads.
 *
 * The resource is keyed on the declaration itself, so a reloaded tree — every
 * node of which arrives as a fresh object off the wire — re-reads the value
 * along with it. That is what lets a write cost one read instead of two: the
 * reconciliation the row needs and the invalidation the pane needs are the
 * same fetch.
 */
const OptionRow: Component<{
  plugin: string;
  path: string[];
  node: PluginOptionNode;
  onChanged: () => void | Promise<unknown>;
}> = (props) => {
  const [value, { mutate, refetch }] = createResource(
    () => props.node,
    () => getPluginOption(props.plugin, props.path),
  );
  const [busy, setBusy] = createSignal(false);

  const editable = () => props.node.writable !== false && !props.node.disabled && !busy();

  const commit = async (next: unknown) => {
    // Optimistic, then reconciled against what the plugin actually stored: a
    // setter is free to normalise, clamp, or reject, and the pane must end up
    // showing the stored value rather than what was typed at it.
    const previous = value();
    const declared = props.node;
    mutate(() => next);
    setBusy(true);
    try {
      await setPluginOption(props.plugin, props.path, next);
      // Sibling options can depend on this one (a `values` or `disabled`
      // function reading it), so the tree is re-read, not just this row.
      await props.onChanged();
      // The reloaded tree replaced this row's declaration, and the value was
      // re-read with it — reading again here would fetch what just arrived.
      // Only a caller that does not reload leaves the row to reconcile itself.
      if (props.node === declared) await refetch();
    } catch (err) {
      mutate(() => previous);
      notificationActions.addNotification('error', `${props.node.name ?? props.path.at(-1)}: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const press = async () => {
    setBusy(true);
    try {
      await executePluginOption(props.plugin, props.path);
      await props.onChanged();
    } catch (err) {
      notificationActions.addNotification('error', `${props.node.name ?? 'Action'}: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="py-1.5" data-testid={`plugin-option-${props.path.join('-')}`}>
      <Show when={props.node.type !== 'execute'}>
        <label class="block text-xs text-shell-body mb-1">
          {props.node.name ?? props.path.at(-1)}
          <Show when={props.node.writable === false}>
            <span class="ml-1 text-[10px] text-muted" title="This setting is read-only">
              (read-only)
            </span>
          </Show>
        </label>
      </Show>

      <Show when={props.node.type === 'toggle'}>
        <input
          type="checkbox"
          class="align-middle"
          checked={value() === true}
          disabled={!editable()}
          onChange={(e) => void commit(e.currentTarget.checked)}
        />
      </Show>

      <Show when={props.node.type === 'select'}>
        <select
          class={inputClass}
          disabled={!editable()}
          value={String(value() ?? '')}
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
          value={value() === null || value() === undefined ? '' : String(value())}
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
          value={value() === null || value() === undefined ? '' : String(value())}
          disabled={!editable()}
          onChange={(e) => {
            const raw = e.currentTarget.value;
            void commit(raw === '' ? null : raw);
          }}
        />
      </Show>

      <Show when={props.node.desc}>
        <p class="mt-1 text-[11px] text-muted leading-snug">{props.node.desc}</p>
      </Show>
    </div>
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
  <div class={props.path.length > 0 ? 'mt-2 pl-2 border-l border-hairline' : ''}>
    <Show when={props.path.length > 0 && props.node.name}>
      <h4 class="text-xs font-medium text-shell-body">{props.node.name}</h4>
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
  </div>
);

export const PluginSettings: Component<{
  plugin: string;
  tree: PluginOptionNode;
  /** Re-read the whole tree; resolves once the reloaded one is in hand. */
  onChanged: () => void | Promise<unknown>;
}> = (props) => (
  <Show
    when={(props.tree.args ?? []).length > 0}
    fallback={<p class="text-xs text-muted">This plugin declares no settings.</p>}
  >
    <div data-testid={`plugin-settings-${props.plugin}`}>
      <OptionGroup plugin={props.plugin} path={[]} node={props.tree} onChanged={props.onChanged} />
    </div>
  </Show>
);
