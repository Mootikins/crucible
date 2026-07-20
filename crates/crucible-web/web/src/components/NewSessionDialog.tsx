import { Component, For, Show, createEffect, createSignal } from 'solid-js';
import { getConfig, listKilns, listProjects } from '@/lib/api';
import type { KilnListEntry, Project } from '@/lib/types';

const basename = (p: string): string => p.replace(/\/$/, '').split('/').pop() ?? p;

/**
 * New-session chooser: pick the kiln (knowledge) and optional project
 * workspace (where work happens) before creating. Defaults are prefilled —
 * Enter accepts them, so the fast path stays one keypress. Emits
 * `crucible:create-session` with the chosen params (SessionContext creates).
 */
export const NewSessionDialog: Component<{
  open: boolean;
  onClose: () => void;
}> = (props) => {
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [kiln, setKiln] = createSignal('');
  const [workspace, setWorkspace] = createSignal('');
  const [loading, setLoading] = createSignal(false);

  createEffect(() => {
    if (!props.open) return;
    setLoading(true);
    void (async () => {
      try {
        const [cfg, ks, ps] = await Promise.all([
          getConfig().catch(() => null),
          listKilns().catch(() => [] as KilnListEntry[]),
          listProjects().catch(() => [] as Project[]),
        ]);
        // The configured default kiln may not be in the registry list yet.
        const all = [...ks];
        const def = cfg?.kiln_path;
        if (def && !all.some((k) => k.path === def)) {
          all.unshift({ path: def, name: basename(def) });
        }
        setKilns(all);
        setProjects(ps);
        setKiln((prev) => prev || def || all[0]?.path || '');
      } finally {
        setLoading(false);
      }
    })();
  });

  const create = () => {
    if (!kiln()) return;
    window.dispatchEvent(
      new CustomEvent('crucible:create-session', {
        detail: { kiln: kiln(), workspace: workspace() || undefined },
      }),
    );
    props.onClose();
  };

  const selectClass =
    'w-full px-2 py-1.5 rounded border border-hairline bg-surface-base text-sm text-shell-body outline-none focus:border-primary';

  return (
    <Show when={props.open}>
      <div class="fixed inset-0 z-[110] bg-black/65" onClick={() => props.onClose()} />
      <div
        class="fixed left-1/2 top-24 z-[120] w-[min(420px,92vw)] -translate-x-1/2 rounded-xl border border-hairline bg-surface-overlay shadow-2xl"
        onKeyDown={(e) => {
          if (e.key === 'Enter') create();
          if (e.key === 'Escape') props.onClose();
        }}
      >
        <div class="flex items-center justify-between border-b border-hairline px-5 py-3">
          <h2 class="text-sm font-semibold text-shell-ink">New Session</h2>
          <button
            onClick={() => props.onClose()}
            class="rounded p-1 text-muted hover:bg-hover-wash hover:text-shell-ink transition-colors"
            aria-label="Close"
          >
            <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="flex flex-col gap-3 p-5 text-sm">
          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-muted-dark">Kiln (knowledge)</span>
            <select
              class={selectClass}
              value={kiln()}
              disabled={loading()}
              onChange={(e) => setKiln(e.currentTarget.value)}
              data-testid="new-session-kiln"
            >
              <For each={kilns()}>
                {(k) => <option value={k.path}>{k.name || basename(k.path)} — {k.path}</option>}
              </For>
            </select>
          </label>

          <label class="flex flex-col gap-1">
            <span class="text-xs uppercase tracking-wider text-muted-dark">
              Project workspace (optional)
            </span>
            <select
              class={selectClass}
              value={workspace()}
              disabled={loading()}
              onChange={(e) => setWorkspace(e.currentTarget.value)}
              data-testid="new-session-workspace"
            >
              <option value="">None — work in the kiln</option>
              <For each={projects()}>
                {(p) => <option value={p.path}>{p.name || basename(p.path)} — {p.path}</option>}
              </For>
            </select>
          </label>
        </div>

        <div class="flex justify-end gap-2 border-t border-hairline px-5 py-3">
          <button
            onClick={() => props.onClose()}
            class="px-3 py-1.5 rounded border border-hairline text-sm text-muted hover:bg-hover-wash hover:text-shell-ink transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={create}
            disabled={!kiln()}
            data-testid="new-session-create"
            class="px-3 py-1.5 rounded bg-primary text-sm text-white hover:bg-primary-hover disabled:opacity-50 transition-colors"
          >
            Create
          </button>
        </div>
      </div>
    </Show>
  );
};
