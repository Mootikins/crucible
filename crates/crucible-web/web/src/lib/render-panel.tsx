import { untrack, type Accessor, type JSX } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { Tab } from '@/windowing';
import { getGlobalRegistry } from './panel-registry';
import { reactiveMetadataProps } from './panel-props';

/**
 * The body of a window tab: the registered panel for its content type.
 *
 * The window manager calls this once per tab identity and content type, and
 * it passes the LIVE tab. The panel props read the live tab through
 * `reactiveMetadataProps`, so a later metadata write reaches a mounted panel
 * without a remount.
 */
export function renderPanel(tab: Accessor<Tab>): JSX.Element {
  const panel = getGlobalRegistry().get(untrack(tab).contentType);
  if (!panel) {
    // Every shipped content type is registry-backed; anything else is a
    // stale persisted layout entry.
    return (
      <div class="flex-1 bg-shell-bg flex items-center justify-center">
        <div class="text-muted-dark text-sm">Unknown content type</div>
      </div>
    );
  }
  return <Dynamic component={panel.component} {...reactiveMetadataProps(tab)} />;
}
