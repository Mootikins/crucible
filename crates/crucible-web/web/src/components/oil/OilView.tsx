import { Component, Show, createResource, createSignal } from 'solid-js';
import { OilNode } from './OilNode';
import type { OilTree } from '@/lib/oil-types';
import { renderPluginView } from '@/lib/api';

/**
 * One live plugin view: fetch a tree, draw it, and re-fetch after an action.
 *
 * The re-fetch is the whole state model, and it is deliberate. A client that
 * applied an action's effect locally would hold a second description of the
 * plugin's state, free to drift from the one `render` produces. Instead the
 * daemon answers an action WITH the new tree (`plugin.view_action` renders
 * after dispatching), so there is exactly one description and one round trip.
 */
export interface OilViewProps {
  plugin: string;
  view: string;
  params?: Record<string, unknown>;
}

export const OilView: Component<OilViewProps> = (props) => {
  const [tree, setTree] = createSignal<OilTree | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const [initial] = createResource(
    () => ({ plugin: props.plugin, view: props.view, params: props.params }),
    async (key) => {
      try {
        const node = await renderPluginView(key.plugin, key.view, key.params);
        setTree(node);
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
      return true;
    },
  );

  const act = async (action: string, params: Record<string, string>) => {
    try {
      const node = await renderPluginView(props.plugin, props.view, params, action);
      setTree(node);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div class="oil-view not-prose my-3" data-testid={`oil-view-${props.plugin}-${props.view}`}>
      <Show when={error()}>
        {(msg) => (
          <div class="text-sm text-danger border border-danger/40 rounded px-3 py-2">
            {props.plugin}/{props.view}: {msg()}
          </div>
        )}
      </Show>
      <Show when={!error() && tree()}>
        {(node) => <OilNode node={node()} onAction={act} />}
      </Show>
      <Show when={!error() && !tree() && initial.loading}>
        <div class="text-sm text-muted italic">loading {props.plugin}/{props.view}…</div>
      </Show>
    </div>
  );
};

export default OilView;
