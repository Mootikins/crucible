import { Component, For, Show } from 'solid-js';
import { usePublication } from './usePublication';
import type { BlockProps } from './registry';

/**
 * What a plugin block looks like before anyone writes a component for it.
 *
 * Reads `<plugin>:<block>` from the publication registry and renders whatever
 * came back. Deliberately plain: this is the floor that keeps a data-only
 * plugin visible, not a layout anyone should be happy with.
 */
export const GenericBlock: Component<BlockProps> = (props) => {
  const value = usePublication<unknown>(props.plugin, `${props.plugin}:${props.block}`);

  return (
    <div class="not-prose my-3 rounded-lg border border-hairline p-3" data-testid="generic-block">
      <div class="pb-2 text-xs font-semibold uppercase tracking-wide text-muted">
        {props.plugin} / {props.block}
      </div>
      <Show
        when={value() !== undefined}
        fallback={
          <div class="text-sm text-muted italic">
            {props.plugin} has published nothing under{' '}
            <code>
              {props.plugin}:{props.block}
            </code>
            .
          </div>
        }
      >
        {/* Not the callback form: `when` here is a boolean, so the callback
            would be handed `true` rather than the published value. */}
        <Rows value={value()} />
      </Show>
    </div>
  );
};

/** A shallow, readable dump. Arrays of objects become a table; anything else JSON. */
const Rows: Component<{ value: unknown }> = (props) => {
  const rows = () => (Array.isArray(props.value) ? props.value : null);
  const keys = () => {
    const r = rows();
    if (!r || r.length === 0 || typeof r[0] !== 'object' || r[0] === null) return null;
    return Object.keys(r[0] as object);
  };

  return (
    <Show
      when={keys()}
      fallback={
        <pre class="overflow-x-auto text-xs">
          {JSON.stringify(props.value, null, 2)}
        </pre>
      }
    >
      {(cols) => (
        <div class="overflow-x-auto">
          <table class="text-sm">
            <thead>
              <tr>
                <For each={cols()}>
                  {(k) => <th class="px-2 py-1 text-left text-muted font-medium">{k}</th>}
                </For>
              </tr>
            </thead>
            <tbody>
              <For each={rows() ?? []}>
                {(row) => (
                  <tr>
                    <For each={cols()}>
                      {(k) => (
                        <td class="px-2 py-1 align-top">
                          {String((row as Record<string, unknown>)[k] ?? '')}
                        </td>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      )}
    </Show>
  );
};
