import { Component, For, Show, createMemo, createSignal } from 'solid-js';
import { usePublication } from './usePublication';
import { runPluginCommand } from '@/lib/api';
import type { BlockProps } from './registry';

/**
 * The kanban board, drawn natively from the plugin's published data.
 *
 * The plugin owns the tickets and publishes `{columns, tickets}`. It describes
 * no layout, no colour and no widget — everything below is this component's
 * decision, which is exactly what the earlier Oil version could not allow.
 *
 * Three things here are impossible to express in an Oil tree, and they are the
 * reason the seam moved:
 *
 *   - **Drag and drop.** A terminal has no gestures to project, so a shared
 *     vocabulary could never carry one.
 *   - **A responsive layout.** The columns wrap and scroll; Oil has no wrap, no
 *     breakpoint and no min-width.
 *   - **Theme colours.** These are the app's own tokens. A Lua-declared tree
 *     can only name one of sixteen terminal colours or a hex literal.
 */

interface Ticket {
  file: string;
  title: string;
  status: string;
}

interface Board {
  columns: string[];
  tickets: Ticket[];
  folder?: string;
  kiln?: string;
  dir?: string;
}

export const KanbanBlock: Component<BlockProps> = (props) => {
  const board = usePublication<Board>(props.plugin, 'kanban:board');
  const [dragging, setDragging] = createSignal<string | null>(null);
  const [over, setOver] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  // Grouping is the renderer's job. The plugin publishes a flat list and the
  // column order, so a board that wanted to group by assignee instead would
  // need no change on the Lua side.
  const grouped = createMemo(() => {
    const b = board();
    if (!b) return [];
    return b.columns.map((name) => ({
      name,
      tickets: b.tickets.filter((t) => t.status === name),
    }));
  });

  const move = async (file: string, to: string) => {
    try {
      const result = await runPluginCommand('kanban_move', {
        file,
        to,
        folder: props.params.folder,
        kiln: props.params.kiln,
      });
      // The plugin republishes on success, which pushes `publication_changed`
      // and re-renders this block. Nothing is applied locally: one description
      // of the board, and the plugin owns it.
      if (result && typeof result === 'object' && (result as { ok?: boolean }).ok === false) {
        setError((result as { error?: string }).error ?? 'the move was refused');
      } else {
        setError(null);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div class="not-prose my-3" data-testid="kanban-block">
      <Show when={error()}>
        {(msg) => (
          <div class="mb-2 text-sm text-danger border border-danger/40 rounded px-3 py-2">
            {msg()}
          </div>
        )}
      </Show>

      <Show
        when={board()}
        fallback={<div class="text-sm text-muted italic">loading the board…</div>}
      >
        {(b) => (
          <Show
            when={b().tickets.length > 0}
            fallback={
              <div class="text-sm text-muted border border-hairline rounded px-3 py-2">
                No tickets in <code>{b().dir ?? b().folder}</code>. A ticket is a{' '}
                <code>.md</code> file with <code>status:</code> in its frontmatter.
              </div>
            }
          >
            {/* Wraps on a narrow viewport, scrolls when it cannot. */}
            <div class="flex flex-wrap gap-3 items-start">
              <For each={grouped()}>
                {(column) => (
                  <div
                    class="flex-1 min-w-[13rem] rounded-lg border p-2 transition-colors"
                    classList={{
                      'border-hairline bg-surface': over() !== column.name,
                      'border-primary bg-primary/5': over() === column.name,
                    }}
                    onDragOver={(e) => {
                      e.preventDefault();
                      setOver(column.name);
                    }}
                    onDragLeave={() => setOver((c) => (c === column.name ? null : c))}
                    onDrop={(e) => {
                      e.preventDefault();
                      setOver(null);
                      const file = dragging();
                      setDragging(null);
                      if (file) void move(file, column.name);
                    }}
                  >
                    <div class="px-1 pb-2 text-xs font-semibold uppercase tracking-wide text-muted">
                      {column.name} ({column.tickets.length})
                    </div>
                    <div class="flex flex-col gap-2">
                      <For each={column.tickets}>
                        {(ticket) => (
                          <div
                            draggable="true"
                            class="rounded-md border border-hairline bg-surface-elevated px-2 py-1.5
                                   text-sm cursor-grab active:cursor-grabbing"
                            classList={{ 'opacity-40': dragging() === ticket.file }}
                            onDragStart={(e) => {
                              setDragging(ticket.file);
                              e.dataTransfer?.setData('text/plain', ticket.file);
                            }}
                            onDragEnd={() => {
                              setDragging(null);
                              setOver(null);
                            }}
                            title={ticket.file}
                          >
                            {ticket.title}
                          </div>
                        )}
                      </For>
                      <Show when={column.tickets.length === 0}>
                        <div class="px-1 py-2 text-xs text-muted italic">nothing here</div>
                      </Show>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>
        )}
      </Show>
    </div>
  );
};

export default KanbanBlock;
