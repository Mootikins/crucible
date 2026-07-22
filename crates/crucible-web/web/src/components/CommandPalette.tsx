import { Component, For, Show, createEffect, createMemo, createResource, createSignal } from 'solid-js';
import { Command } from 'cmdk-solid';
import { statusBarStore } from '@/stores/statusBarStore';
import { listNotes } from '@/lib/api';
import { openFileInEditor } from '@/lib/file-actions';
import { noteAbsolutePath } from '@/lib/note-actions';
import { fuzzyScore } from '@/lib/fuzzy';

export type CommandCategory = 'Chat' | 'Session' | 'Navigation' | 'Settings';

export interface PaletteCommand {
  id: string;
  label: string;
  description?: string;
  shortcut?: string;
  category: CommandCategory;
  keywords?: string[];
  action: () => void;
}

/** What the palette opens as: Ctrl+P = commands, Ctrl+O = notes. */
export type PaletteMode = 'commands' | 'notes';

interface CommandPaletteProps {
  open: boolean;
  commands: PaletteCommand[];
  onOpenChange: (open: boolean) => void;
  /** Initial scope. Typing a prefix (`>` commands / `[[` notes) crosses over. */
  mode?: PaletteMode;
}

// ── Palette ──────────────────────────────────────────────────────────────
// Two single-purpose surfaces sharing one component: the command palette
// (actions only) and the note quick switcher (notes only). No mixed
// results — sessions live in the sessions panel's search, files/notes are
// Ctrl+O. Prefixes are crossover escapes: `[[` from commands jumps to note
// search, `>` from notes jumps to commands.

type OmniKind = 'NOTE' | 'CMD';

interface OmniItem {
  id: string;
  kind: OmniKind;
  label: string;
  description?: string;
  shortcut?: string;
  keywords?: string[];
  action: () => void;
}

const KIND_STYLE: Record<OmniKind, string> = {
  NOTE: 'text-muted border-muted/60',
  CMD: 'text-precog border-precog/60',
};

/** Untyped list caps. Commands are few — show them all; notes cap at a
 * screenful of the most recently updated. Typing re-ranks over everything. */
const IDLE_LIMITS: Record<OmniKind, number> = { NOTE: 20, CMD: 100 };

export function parseOmniQuery(
  raw: string,
  mode: PaletteMode = 'commands',
): { kind: OmniKind; query: string } {
  if (raw.startsWith('>')) return { kind: 'CMD', query: raw.slice(1).trim() };
  if (raw.startsWith('[[')) return { kind: 'NOTE', query: raw.slice(2).trim() };
  return { kind: mode === 'notes' ? 'NOTE' : 'CMD', query: raw.trim() };
}

/**
 * Score an item for the query: label hits dominate (a command whose *name*
 * matches beats one where only a keyword does), otherwise the best score
 * across description/shortcut/keywords counts. Null = filtered out.
 */
function scoreItem(item: OmniItem, query: string): number | null {
  if (!query) return 0;
  const label = fuzzyScore(item.label, query);
  const rest = fuzzyScore(
    [item.description ?? '', item.shortcut ?? '', ...(item.keywords ?? [])].join(' '),
    query
  );
  if (label === null && rest === null) return null;
  return Math.max(label !== null ? label + 2000 : -Infinity, rest ?? -Infinity);
}

export const CommandPalette: Component<CommandPaletteProps> = (props) => {
  const [query, setQuery] = createSignal('');

  createEffect(() => {
    // Track open state; always start a session of the palette clean.
    void props.open;
    setQuery('');
  });

  // Notes load lazily when the palette opens (and re-fetch per open so
  // freshly written notes appear without a reload).
  const [notes] = createResource(
    () => (props.open ? statusBarStore.kilnPath() : null),
    (kiln) => listNotes(kiln).catch(() => [])
  );

  const noteItems = (): OmniItem[] =>
    (notes() ?? [])
      // Recency-first so the untyped switcher shows what you touched last.
      .slice()
      .sort((a, b) => (b.updated_at ?? '').localeCompare(a.updated_at ?? ''))
      .map((note) => ({
        id: `note-${note.path}`,
        kind: 'NOTE' as const,
        label: note.title || note.name,
        description: note.path,
        // Path segments count for fuzzy matching (find "meta arch" style).
        keywords: [...(note.tags ?? []), ...note.path.split('/')],
        // Note records carry kiln-relative paths; the file API is absolute.
        action: () =>
          openFileInEditor(
            noteAbsolutePath(note.path, statusBarStore.kilnPath() ?? ''),
            note.name,
          ),
      }));

  const commandItems = (): OmniItem[] =>
    props.commands.map((command) => ({
      id: command.id,
      kind: 'CMD' as const,
      label: command.label,
      description: command.description,
      shortcut: command.shortcut,
      keywords: [...(command.keywords ?? []), command.category],
      action: command.action,
    }));

  const visible = createMemo(() => {
    const { kind, query: q } = parseOmniQuery(query(), props.mode ?? 'commands');
    const source = kind === 'NOTE' ? noteItems() : commandItems();
    let items = source
      .map((item) => ({ item, score: scoreItem(item, q) }))
      .filter((x): x is { item: OmniItem; score: number } => x.score !== null)
      .sort((a, b) => b.score - a.score)
      .map((x) => x.item);
    if (!q) items = items.slice(0, IDLE_LIMITS[kind]);
    return { kind, items };
  });

  const placeholder = () =>
    (props.mode ?? 'commands') === 'notes'
      ? 'Open note… ( > to run a command )'
      : 'Run a command… ( [[ to open a note )';

  return (
    <Command.Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      label={(props.mode ?? 'commands') === 'notes' ? 'Note switcher' : 'Command palette'}
      class="fixed left-1/2 top-24 z-[120] w-[min(680px,92vw)] -translate-x-1/2 overflow-hidden rounded-xl border border-hairline-strong bg-surface-elevated/95 shadow-2xl backdrop-blur cru-anim-pop"
      overlayClassName="fixed inset-0 z-[110] bg-black/65 cru-anim-fade"
      shouldFilter={false}
      loop
    >
      <Command.Input
        value={query()}
        onValueChange={setQuery}
        placeholder={placeholder()}
        class="w-full border-b border-hairline bg-transparent px-4 py-3 text-sm text-shell-ink outline-none placeholder:text-muted-dark"
      />

      <Command.List class="max-h-[60vh] overflow-y-auto p-1.5">
        <Show
          when={visible().items.length > 0}
          fallback={
            <div class="px-3 py-8 text-center text-sm text-muted-dark">
              Nothing matches "{query()}".
            </div>
          }
        >
          <Command.Group class="mb-1">
            <For each={visible().items}>
              {(item) => (
                <Command.Item
                  value={`${item.kind}:${item.id}`}
                  onSelect={() => {
                    item.action();
                    props.onOpenChange(false);
                  }}
                  class="group flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-shell-body outline-none aria-selected:bg-primary/15 aria-selected:text-shell-ink data-[selected=true]:bg-primary/15 data-[selected=true]:text-shell-ink"
                >
                  <span
                    class={`font-mono text-[10px] font-medium border rounded-[3px] px-1 py-px w-[54px] text-center flex-none opacity-85 ${KIND_STYLE[item.kind]}`}
                  >
                    {item.kind}
                  </span>
                  <div class="min-w-0 flex-1">
                    <div class="text-[13px] leading-5 truncate">{item.label}</div>
                    <Show when={item.description}>
                      <div class="text-xs text-muted-dark truncate">{item.description}</div>
                    </Show>
                  </div>
                  <Show when={item.shortcut}>
                    <kbd class="rounded border border-hairline bg-surface-overlay px-1.5 py-0.5 text-[10px] text-muted">
                      {item.shortcut}
                    </kbd>
                  </Show>
                </Command.Item>
              )}
            </For>
          </Command.Group>
        </Show>
      </Command.List>

      <div class="flex gap-3.5 border-t border-hairline bg-shell-panel px-4 py-2 font-mono text-[10.5px] text-muted-dark">
        <span><span class="text-primary">&gt;</span> command</span>
        <span><span class="text-primary">[[</span> note</span>
        <span class="ml-auto">Ctrl+P commands · Ctrl+O notes</span>
      </div>
    </Command.Dialog>
  );
};
