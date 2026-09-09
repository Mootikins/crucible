import type { Component } from 'solid-js';

/**
 * The web half of the plugin contract.
 *
 * A plugin owns data on the daemon and publishes it. This registry maps a
 * `plugin/block` name to the component that draws that data *natively* — real
 * drag targets, real breakpoints, real focus. Nothing here receives a layout
 * from the plugin, which is the whole point: a terminal's vocabulary is not
 * the browser's, and neither should be the contract.
 *
 * ## Why the registry is compiled in, for now
 *
 * These components ship with the app rather than with the plugin, because a
 * plugin bundle the daemon serves is `script-src 'self'` — the CSP would admit
 * it and protect nothing, and the service worker's root scope leans on that
 * same bound. Third-party block code needs a sandboxed opaque origin and a
 * postMessage bridge before it can be loaded. Until that exists, a plugin that
 * wants a custom web block contributes it here; a plugin that does not still
 * renders through the generic fallback, from its published data alone.
 */
export interface BlockProps {
  /** The publishing plugin, from the fence's first line. */
  plugin: string;
  /** The block name, from the fence's first line. */
  block: string;
  /** Whatever JSON followed the first line. The plugin's own vocabulary. */
  params: Record<string, unknown>;
}

const registry = new Map<string, Component<BlockProps>>();

/** Register the component for one `plugin/block` pair. */
export function registerBlock(plugin: string, block: string, component: Component<BlockProps>) {
  registry.set(`${plugin}/${block}`, component);
}

/** The component for a `plugin/block` pair, or undefined for the fallback. */
export function lookupBlock(plugin: string, block: string): Component<BlockProps> | undefined {
  return registry.get(`${plugin}/${block}`);
}
