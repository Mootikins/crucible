import { Component, Show, createMemo } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { lookupBlock, type BlockProps } from './registry';
import { GenericBlock } from './GenericBlock';
import './register-blocks';

/**
 * One plugin block: the registered component for `plugin/block`, or the
 * generic renderer when none is registered.
 *
 * The fallback is deliberate and load-bearing. A plugin that publishes data
 * and ships no web component still appears — as a plain readable table of what
 * it published — so a custom block is an upgrade rather than a prerequisite.
 * Without it the ecosystem splits by surface: plugins that shipped TS are
 * visible, plugins that did not are silently absent.
 */
export const PluginBlock: Component<BlockProps> = (props) => {
  const component = createMemo(() => lookupBlock(props.plugin, props.block));
  return (
    <Show when={component()} fallback={<GenericBlock {...props} />}>
      {(c) => <Dynamic component={c()} {...props} />}
    </Show>
  );
};
