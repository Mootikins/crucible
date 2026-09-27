import { render } from 'solid-js/web';
import { BaseView } from './BaseView';
export function mountBases(host: HTMLElement, path?: string, kiln?: string): () => void {
  const disposers: (() => void)[] = [];
  for (const node of host.querySelectorAll<HTMLElement>('.base-mount')) {
    const encoded = node.getAttribute('data-base-yaml');
    let yaml: string | undefined;
    try { yaml = encoded === null ? undefined : decodeURIComponent(encoded); } catch { node.textContent = 'Invalid base source'; continue; }
    const file = node.getAttribute('data-base-path') ?? undefined;
    const view = node.getAttribute('data-base-view') ?? undefined;
    disposers.push(render(() => BaseView({ yaml, filePath: file, view, host: path, kiln }), node));
  }
  return () => disposers.forEach(dispose => dispose());
}
