/** The path of a note under its root, with the note's own name in bold. */
import { For, Show, type Component } from 'solid-js';
import { scrollFade } from '@/lib/scroll-fade';
import type { Project } from '@/lib/types';

export type BreadcrumbProject = Pick<Project, 'path' | 'name'>;

/** Display roots never alter the file's identity or its wikilink resolution. */
export function fileBreadcrumb(
  path: string,
  kiln?: string,
  projects: readonly BreadcrumbProject[] = [],
) {
  const roots = projects.map((project) => ({
    path: project.path.replace(/\/+$/, ''), name: project.name, kind: 'Project',
  }));
  if (kiln) roots.push({
    path: kiln.replace(/\/+$/, ''),
    name: kiln.replace(/\/+$/, '').split('/').pop() || 'Kiln', kind: 'Kiln',
  });
  const root = roots.filter((entry) => entry.path && (path === entry.path || path.startsWith(`${entry.path}/`)))
    .sort((a, b) => b.path.length - a.path.length || (a.kind === 'Kiln' ? -1 : 1))[0];
  if (root) return { root: root.name, path: path.slice(root.path.length).replace(/^\//, ''), kind: root.kind };
  // A roster may still be loading, or the file may lie outside registered roots.
  return { root: '', path: path.startsWith('/') ? path.split('/').pop() || '' : path, kind: undefined };
}

export const Breadcrumb: Component<{ root: string; path: string; kind?: string }> = (props) => {
  const parts = () => props.path.split('/').filter(Boolean);
  return (
    <span class="note-breadcrumb" data-testid="note-breadcrumb" ref={scrollFade('x')}>
      <Show when={props.root}><span title={props.kind ? `${props.kind}: ${props.root}` : undefined}>{props.root}</span></Show>
      <For each={parts()}>
        {(p, i) => (
          <>
            <Show when={props.root || i() > 0}><span class="note-breadcrumb-separator">/</span></Show>
            {i() === parts().length - 1 ? <b>{p}</b> : p}
          </>
        )}
      </For>
    </span>
  );
};
