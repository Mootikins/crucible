import { createResource, Show, type Component } from 'solid-js';
import { Dynamic } from 'solid-js/web';

// Lazy modules keep a dynamic formula icon from loading the entire Lucide barrel.
const icons = import.meta.glob<{ default: Component<{ size?: number; 'aria-label'?: string }> }>('/node_modules/lucide-solid/dist/source/icons/*.jsx');
export const BaseIcon: Component<{ name: string }> = props => {
  const [icon] = createResource(() => props.name.replace(/^lucide-/, ''), async name => {
    const load = icons[`/node_modules/lucide-solid/dist/source/icons/${name}.jsx`];
    return load ? (await load()).default : undefined;
  });
  return <Show when={icon()} fallback={<span>{props.name}</span>}>{component => <Dynamic component={component()} size={16} aria-label={props.name} />}</Show>;
};
