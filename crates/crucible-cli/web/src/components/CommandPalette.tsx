import { Component, For, Show, createEffect, createMemo, createResource, createSignal } from 'solid-js';
import { Command } from 'cmdk-solid';
import { useSessionSafe } from '@/contexts/SessionContext';
import { statusBarStore } from '@/stores/statusBarStore';
import { shellActions } from '@/stores/shellStore';
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

interface CommandPaletteProps {
  open: boolean;
  commands: PaletteCommand[];
  onOpenChange: (open: boolean) => void;
}

// ── Omnibox ──────────────────────────────────────────────────────────────
// One palette that goes anywhere (Feature Spec §2.2, Crucible Shell design
// turn 5): GO surfaces, note quick-switcher, sessions, and commands, with
// type-ahead prefixes — `>` commands only, `[[` notes only.

type OmniKind = 'GO' | 'NOTE' | 'SESSION' | 'CMD';

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
  GO: 'text-primary border-primary/70',
  NOTE: 'text-muted border-muted/60',
  SESSION: 'text-attention border-attention/60',
  CMD: 'text-precog border-precog/60',
};

const KIND_ORDER: OmniKind[] = ['GO', 'NOTE', 'SESSION', 'CMD'];

/** How many items a section shows before the user types anything. */
const IDLE_LIMITS: Record<OmniKind, number> = { GO: 8, NOTE: 5, SESSION: 4, CMD: 6 };

export function parseOmniQuery(raw: string): { kinds: OmniKind[] | null; query: string } {
  if (raw.startsWith('>')) return { kinds: ['CMD'], query: raw.slice(1).trim() };
  if (raw.startsWith('[[')) return { kinds: ['NOTE'], query: raw.slice(2).trim() };
  return { kinds: null, query: raw.trim() };
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
  const sessionCtx = useSessionSafe();

  createEffect(() => {
    if (!props.open) {
      setQuery('');
    }
  });

  // Notes load lazily when the palette opens (and re-fetch per open so
  // freshly written notes appear without a reload).
  const [notes] = createResource(
    () => (props.open ? statusBarStore.kilnPath() : null),
    (kiln) => listNotes(kiln).catch(() => [])
  );

  const goItems = (): OmniItem[] => {
    const sessionTitle = statusBarStore.activeSessionTitle();
    return [
      { id: 'go-home', kind: 'GO', label: 'Home', keywords: ['landing', 'start'], action: () => shellActions.goHome() },
      { id: 'go-inbox', kind: 'GO', label: 'Inbox', keywords: ['pending', 'approvals', 'waiting'], action: () => shellActions.goInbox() },
      { id: 'go-edit', kind: 'GO', label: 'Editor (✎ Edit mode)', keywords: ['edit', 'notes', 'vault'], action: () => shellActions.goEdit() },
      {
        id: 'go-session',
        kind: 'GO',
        label: sessionTitle ? `Session: ${sessionTitle}` : 'Session (◆ Session mode)',
        keywords: ['chat', 'agent'],
        action: () => shellActions.goSession(),
      },
    ];
  };

  const noteItems = (): OmniItem[] =>
    (notes() ?? []).map((note) => ({
      id: `note-${note.path}`,
      kind: 'NOTE' as const,
      label: note.title || note.name,
      keywords: note.tags,
      // Note records carry kiln-relative paths; the file API is absolute.
      action: () =>
        openFileInEditor(
          noteAbsolutePath(note.path, statusBarStore.kilnPath() ?? ''),
          note.name,
        ),
    }));

  const sessionItems = (): OmniItem[] =>
    sessionCtx
      .sessions()
      .filter((s) => !s.archived)
      .map((session) => ({
        id: `session-${session.id}`,
        kind: 'SESSION' as const,
        label: session.title || `Session ${session.id.slice(0, 8)}`,
        description: session.agent_model ?? undefined,
        keywords: ['resume', 'session'],
        action: () => void sessionCtx.selectSession(session.id).catch(() => {}),
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

  const grouped = createMemo(() => {
    const { kinds, query: q } = parseOmniQuery(query());
    const sections: Record<OmniKind, OmniItem[]> = {
      GO: goItems(),
      NOTE: noteItems(),
      SESSION: sessionItems(),
      CMD: commandItems(),
    };
    return KIND_ORDER.filter((kind) => !kinds || kinds.includes(kind))
      .map((kind) => {
        let items = sections[kind]
          .map((item) => ({ item, score: scoreItem(item, q) }))
          .filter((x): x is { item: OmniItem; score: number } => x.score !== null)
          .sort((a, b) => b.score - a.score)
          .map((x) => x.item);
        if (!q) items = items.slice(0, IDLE_LIMITS[kind]);
        return { kind, items };
      })
      .filter((group) => group.items.length > 0);
  });

  const hasResults = createMemo(() => grouped().length > 0);

  return (
    <Command.Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      label="Omnibox"
      class="fixed left-1/2 top-24 z-[120] w-[min(680px,92vw)] -translate-x-1/2 overflow-hidden rounded-xl border border-white/[0.12] bg-surface-elevated/95 shadow-2xl backdrop-blur"
      overlayClassName="fixed inset-0 z-[110] bg-black/65"
      shouldFilter={false}
      loop
    >
      <Command.Input
        value={query()}
        onValueChange={setQuery}
        placeholder="Go anywhere… ( > command · [[ note )"
        class="w-full border-b border-white/[0.08] bg-transparent px-4 py-3 text-sm text-shell-ink outline-none placeholder:text-muted-dark"
      />

      <Command.List class="max-h-[60vh] overflow-y-auto p-1.5">
        <Show
          when={hasResults()}
          fallback={
            <div class="px-3 py-8 text-center text-sm text-muted-dark">
              Nothing matches "{query()}".
            </div>
          }
        >
          <For each={grouped()}>
            {(group) => (
              <Command.Group class="mb-1">
                <For each={group.items}>
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
                        class={`font-mono text-[9.5px] font-medium border rounded-[3px] px-1 py-px w-[54px] text-center flex-none opacity-85 ${KIND_STYLE[item.kind]}`}
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
                        <kbd class="rounded border border-white/10 bg-surface-overlay px-1.5 py-0.5 text-[10px] text-muted">
                          {item.shortcut}
                        </kbd>
                      </Show>
                    </Command.Item>
                  )}
                </For>
              </Command.Group>
            )}
          </For>
        </Show>
      </Command.List>

      <div class="flex gap-3.5 border-t border-white/[0.08] bg-shell-panel px-4 py-2 font-mono text-[10.5px] text-muted-dark">
        <span><span class="text-primary">&gt;</span> command</span>
        <span><span class="text-primary">[[</span> note</span>
        <span><span class="text-primary">◆</span> session</span>
        <span><span class="text-primary">⌂</span> go</span>
      </div>
    </Command.Dialog>
  );
};
