import { Component, For, Show, createEffect, createMemo, createSignal } from 'solid-js';
import { Command } from 'cmdk-solid';

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

const CATEGORY_ORDER: CommandCategory[] = ['Chat', 'Session', 'Navigation', 'Settings'];

function matchesQuery(command: PaletteCommand, query: string): boolean {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;

  const haystack = [
    command.label,
    command.description ?? '',
    command.shortcut ?? '',
    ...(command.keywords ?? []),
  ]
    .join(' ')
    .toLowerCase();

  return haystack.includes(normalized);
}

export const CommandPalette: Component<CommandPaletteProps> = (props) => {
  const [query, setQuery] = createSignal('');

  createEffect(() => {
    if (!props.open) {
      setQuery('');
    }
  });

  const groupedCommands = createMemo(() => {
    return CATEGORY_ORDER.map((category) => ({
      category,
      commands: props.commands.filter((command) => command.category === category && matchesQuery(command, query())),
    })).filter((group) => group.commands.length > 0);
  });

  const hasResults = createMemo(() => groupedCommands().length > 0);

  return (
    <Command.Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      label="Global Command Palette"
      class="fixed left-1/2 top-24 z-[120] w-[min(680px,92vw)] -translate-x-1/2 overflow-hidden rounded-xl border border-zinc-700/80 bg-zinc-900/95 shadow-2xl backdrop-blur"
      overlayClassName="fixed inset-0 z-[110] bg-black/65"
      filter={(value, search, keywords) => {
        const text = `${value} ${(keywords ?? []).join(' ')}`.toLowerCase();
        return text.includes(search.toLowerCase().trim()) ? 1 : 0;
      }}
      loop
    >
      <Command.Input
        value={query()}
        onValueChange={setQuery}
        placeholder="Type a command or search..."
        class="w-full border-b border-zinc-800 bg-zinc-900/95 px-4 py-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-500"
      />

      <Command.List class="max-h-[60vh] overflow-y-auto p-2">
        <Show when={hasResults()} fallback={<Command.Empty class="px-3 py-8 text-center text-sm text-zinc-500">No commands match "{query()}".</Command.Empty>}>
          <For each={groupedCommands()}>
            {(group) => (
              <Command.Group heading={group.category} class="mb-1">
                <div cmdk-group-heading="" class="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wide text-zinc-500">
                  {group.category}
                </div>
                <For each={group.commands}>
                  {(command) => (
                    <Command.Item
                      value={command.label}
                      keywords={command.keywords ?? []}
                      onSelect={() => {
                        command.action();
                        props.onOpenChange(false);
                      }}
                      class="group flex cursor-pointer items-start gap-3 rounded-md px-3 py-2 text-zinc-200 outline-none aria-selected:bg-zinc-800 aria-selected:text-white data-[selected=true]:bg-zinc-800 data-[selected=true]:text-white"
                    >
                      <div class="min-w-0 flex-1">
                        <div class="text-sm font-medium leading-5">{command.label}</div>
                        <Show when={command.description}>
                          <div class="text-xs text-zinc-500 group-data-[selected=true]:text-zinc-300">{command.description}</div>
                        </Show>
                      </div>
                      <Show when={command.shortcut}>
                        <kbd class="rounded border border-zinc-700 bg-zinc-800 px-1.5 py-0.5 text-[10px] text-zinc-400">
                          {command.shortcut}
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
    </Command.Dialog>
  );
};
