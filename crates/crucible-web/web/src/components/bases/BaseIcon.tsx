import { createResource, Show, type Component } from 'solid-js';
import { Dynamic } from 'solid-js/web';

export const BaseIcon: Component<{ name: string }> = props => {
  const [icon] = createResource(() => props.name.replace(/^lucide-/, ''), async name => {
    const { icons } = await import('./icons');
    return icons[`/node_modules/lucide-solid/dist/source/icons/${name}.jsx`];
  });
  return <Show when={icon()} fallback={<span>{props.name}</span>}>{component => <Dynamic component={component()} size={16} aria-label={props.name} />}</Show>;
};
