import type { Accessor } from 'solid-js';
import { createComponent, render } from 'solid-js/web';
import { BaseView, type BaseViewProps } from './BaseView';

/**
 * Where one base comes from, as each host of a base knows it. A value can be
 * empty text: a placeholder writes each attribute, and a block reads each
 * parameter, whether or not the author gave it.
 */
export interface BaseSource {
  /** An inline definition (a `base` fence). */
  yaml?: string;
  /** A saved `.base` file, absolute or relative to the kiln. */
  filePath?: string;
  /** The named view. Empty means the first view of the base. */
  view?: string;
  /** The note that shows the base: its `this`, and the key to its kiln. */
  host?: string;
  /** The kiln by name or path. An accessor, when the host can change it. */
  kiln?: string | Accessor<string | undefined>;
}

/** Empty text is "not given". */
function given(value: string | undefined): string | undefined {
  return value === undefined || value === '' ? undefined : value;
}

/**
 * The props of `BaseView` for one source. Each host builds its base through
 * this, so an empty view name reaches the daemon as no view (the first one)
 * and not as a request for a view named "".
 */
export function baseViewProps(source: BaseSource): BaseViewProps {
  const kiln = source.kiln;
  return {
    yaml: source.yaml,
    filePath: given(source.filePath),
    view: given(source.view),
    host: given(source.host),
    get kiln() {
      return given(typeof kiln === 'function' ? kiln() : kiln);
    },
  };
}

/** Renders one base into `node`. The answer removes it again. */
export function mountBaseView(node: HTMLElement, source: BaseSource): () => void {
  return render(() => createComponent(BaseView, baseViewProps(source)), node);
}

/**
 * Mounts a base into each `.base-mount` placeholder of the reading view.
 * `path` is the note that shows them, and `kiln` its kiln.
 */
export function mountBases(host: HTMLElement, path?: string, kiln?: string): () => void {
  const disposers: (() => void)[] = [];
  for (const node of host.querySelectorAll<HTMLElement>('.base-mount')) {
    const encoded = node.getAttribute('data-base-yaml');
    let yaml: string | undefined;
    try { yaml = encoded === null ? undefined : decodeURIComponent(encoded); } catch { node.textContent = 'Invalid base source'; continue; }
    disposers.push(mountBaseView(node, {
      yaml,
      filePath: node.getAttribute('data-base-path') ?? undefined,
      view: node.getAttribute('data-base-view') ?? undefined,
      host: path,
      kiln,
    }));
  }
  return () => disposers.forEach(dispose => dispose());
}
