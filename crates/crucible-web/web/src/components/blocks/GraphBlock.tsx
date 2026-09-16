import { Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { runPluginCommand } from '@/lib/api';
import { useKilns } from '@/lib/query/kilns';
import { kilnForPath, noteAbsolutePath } from '@/lib/note-actions';
import { openFileInEditor } from '@/lib/file-actions';
import { useEditorSafe } from '@/contexts/EditorContext';
import type { BlockProps } from './registry';

/**
 * The focused note's neighbourhood, read on the daemon at a depth the user
 * moves.
 *
 * This block is the other half of `runtime/plugins/graph/`, and it exists to
 * answer a different question from `KanbanBlock`. Kanban draws **published**
 * data: the plugin decides when the board changed and pushes it. There is no
 * publication here and there cannot be one, because the answer depends on two
 * things the daemon does not know — which note has focus, and where the user
 * has left the depth control. So the block **invokes a command** and waits.
 *
 * That is the graph tradeoff in `docs/Meta/Analysis/Plugin API Plan.md`:
 * a parameterised read, over RPC, re-issued every time an argument
 * moves. The elapsed time of each read is drawn in the header rather than
 * hidden, because the cost of that shape is the finding, not a detail.
 *
 * `GraphPanel` is untouched and still draws the whole-kiln force view from
 * `GET /api/kiln/graph`. This is not its replacement; it is the smaller,
 * narrower read that endpoint cannot serve.
 */

interface Ring {
  hops: number;
  paths: string[];
}

interface Neighborhood {
  root: string;
  depth: number;
  total: number;
  truncated: boolean;
  rings: Ring[];
  error?: string | null;
}

/** What the plugin will accept; mirrors `MAX_DEPTH` in its `init.luau`. */
const MAX_DEPTH = 4;

/** A parsed positive integer from the fence's params, or undefined. */
function paramNumber(value: unknown): number | undefined {
  const n = typeof value === 'number' ? value : Number(value);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : undefined;
}

/** The last path segment, which is what a reader recognises. */
function leaf(path: string): string {
  return path.split('/').pop() || path;
}

export const GraphBlock: Component<BlockProps> = (props) => {
  const editor = useEditorSafe();
  const kilns = useKilns();
  const [depth, setDepth] = createSignal(paramNumber(props.params.depth) ?? 1);
  const [elapsed, setElapsed] = createSignal<number | null>(null);

  /**
   * The kiln root holding the focused file, derived from that file's own path.
   *
   * The daemon binds `cru.kiln.*` to whichever kiln is open, so a note in a
   * different kiln comes back with an empty neighbourhood rather than an
   * error. Stripping the prefix here is still right: the plugin takes the
   * kiln-relative path a note record carries, and the editor addresses files
   * absolutely.
   */
  const kiln = createMemo(() => {
    const file = editor.activeFile();
    if (!file) return undefined;
    return kilnForPath(file, kilns.data ?? []);
  });

  /** The note to read around: the fence's own, else whatever has focus. */
  const notePath = createMemo(() => {
    const fromFence = props.params.path;
    if (typeof fromFence === 'string' && fromFence) return fromFence;
    const file = editor.activeFile();
    if (!file) return undefined;
    const root = kiln();
    if (root && file.startsWith(`${root}/`)) return file.slice(root.length + 1);
    return file;
  });

  const [answer] = createResource(
    // Both arguments in the source, so a change to either re-issues the read.
    // That is the point of the block: the depth control is a per-move RPC.
    () => {
      const path = notePath();
      return path ? { path, depth: depth() } : undefined;
    },
    async (args): Promise<Neighborhood> => {
      const started = performance.now();
      try {
        // Third argument: this block declares itself as the plugin it draws
        // for, the same as `KanbanBlock`. Without it the call defaults to
        // `APP_CALLER` and the block is indistinguishable from the app, so
        // the route's per-plugin comparison never runs in production.
        return (await runPluginCommand(
          'graph_neighborhood',
          args,
          props.plugin,
        )) as Neighborhood;
      } finally {
        setElapsed(performance.now() - started);
      }
    },
  );

  const open = (path: string) => {
    const root = kiln();
    openFileInEditor(root ? noteAbsolutePath(path, root) : path, leaf(path));
  };

  return (
    <div class="not-prose my-3" data-testid="graph-block">
      <div class="flex flex-wrap items-center gap-3 mb-2">
        <div class="text-xs font-semibold uppercase tracking-wide text-muted">
          <Show when={notePath()} fallback={<span>no note in focus</span>}>
            {(path) => <span title={path()}>{leaf(path())}</span>}
          </Show>
        </div>

        <label class="flex items-center gap-2 text-xs text-muted">
          depth
          <input
            type="range"
            min="1"
            max={MAX_DEPTH}
            step="1"
            value={depth()}
            data-testid="graph-depth"
            aria-label="Neighbourhood depth"
            onInput={(e) => setDepth(Number(e.currentTarget.value))}
          />
          <span class="tabular-nums w-3">{depth()}</span>
        </label>

        <Show when={elapsed() !== null}>
          <span class="text-xs text-muted tabular-nums" data-testid="graph-latency">
            {Math.round(elapsed() ?? 0)} ms
          </span>
        </Show>
      </div>

      <Show
        when={answer()}
        fallback={
          <div class="text-sm text-muted italic">
            <Show when={notePath()} fallback="Open a note to see its neighbourhood.">
              reading the neighbourhood…
            </Show>
          </div>
        }
      >
        {(n) => (
          <Show
            when={!n().error}
            fallback={
              <div class="text-sm text-danger border border-danger/40 rounded px-3 py-2">
                {n().error}
              </div>
            }
          >
            <Show
              when={n().total > 0}
              fallback={
                <div class="text-sm text-muted border border-hairline rounded px-3 py-2">
                  Nothing links to or from <code>{leaf(n().root)}</code> within {n().depth} hop
                  {n().depth === 1 ? '' : 's'}.
                </div>
              }
            >
              <div class="flex flex-col gap-2">
                <For each={n().rings}>
                  {(ring) => (
                    <Show when={ring.paths.length > 0}>
                      <div class="rounded-lg border border-hairline bg-surface p-2">
                        <div class="px-1 pb-1.5 text-xs font-semibold uppercase tracking-wide text-muted">
                          {ring.hops} hop{ring.hops === 1 ? '' : 's'} ({ring.paths.length})
                        </div>
                        <div class="flex flex-wrap gap-1.5">
                          <For each={ring.paths}>
                            {(path) => (
                              <button
                                type="button"
                                data-note={path}
                                title={path}
                                class="rounded-md border border-hairline bg-surface-elevated px-2 py-1
                                       text-sm hover:border-primary transition-colors"
                                onClick={() => open(path)}
                              >
                                {leaf(path)}
                              </button>
                            )}
                          </For>
                        </div>
                      </div>
                    </Show>
                  )}
                </For>
                <Show when={n().truncated}>
                  <div class="text-xs text-muted italic">
                    Cut short at {n().total} notes. Lower the depth, or raise{' '}
                    <code>[plugins.graph] limit</code>.
                  </div>
                </Show>
              </div>
            </Show>
          </Show>
        )}
      </Show>
    </div>
  );
};
