import { Component, createSignal, For, Show, createEffect, onCleanup } from 'solid-js';
import { useProject } from '@/contexts/ProjectContext';
import { useSession } from '@/contexts/SessionContext';

const ChevronDownIcon: Component<{ class?: string }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class={props.class ?? 'w-4 h-4'}>
    <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
  </svg>
);

const PlusIcon: Component<{ class?: string }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class={props.class ?? 'w-4 h-4'}>
    <path d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z" />
  </svg>
);

const FolderIcon: Component<{ class?: string }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class={props.class ?? 'w-4 h-4'}>
    <path d="M3.75 3A1.75 1.75 0 002 4.75v3.26a3.235 3.235 0 011.75-.51h12.5c.644 0 1.245.188 1.75.51V6.75A1.75 1.75 0 0016.25 5h-4.836a.25.25 0 01-.177-.073L9.823 3.513A1.75 1.75 0 008.586 3H3.75zM3.75 9A1.75 1.75 0 002 10.75v4.5c0 .966.784 1.75 1.75 1.75h12.5A1.75 1.75 0 0018 15.25v-4.5A1.75 1.75 0 0016.25 9H3.75z" />
  </svg>
);

const ChatBubbleIcon: Component<{ class?: string }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class={props.class ?? 'w-4 h-4'}>
    <path fill-rule="evenodd" d="M10 2c-2.236 0-4.43.18-6.57.524C1.993 2.755 1 4.014 1 5.426v5.148c0 1.413.993 2.67 2.43 2.902.848.137 1.705.248 2.57.331v3.443a.75.75 0 001.28.53l3.58-3.579a.78.78 0 01.527-.224 41.202 41.202 0 005.183-.5c1.437-.232 2.43-1.49 2.43-2.903V5.426c0-1.413-.993-2.67-2.43-2.902A41.289 41.289 0 0010 2zm0 7a1 1 0 100-2 1 1 0 000 2zM8 8a1 1 0 11-2 0 1 1 0 012 0zm5 1a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
  </svg>
);

const SearchIcon: Component<{ class?: string }> = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class={props.class ?? 'w-4 h-4'}>
    <path fill-rule="evenodd" d="M9 3.5a5.5 5.5 0 100 11 5.5 5.5 0 000-11zM2 9a7 7 0 1112.452 4.391l3.328 3.329a.75.75 0 11-1.06 1.06l-3.329-3.328A7 7 0 012 9z" clip-rule="evenodd" />
  </svg>
);

const SlashIcon: Component = () => (
  <span class="text-neutral-600 mx-1 select-none">/</span>
);

interface DropdownProps<T> {
  items: T[];
  selected: T | null;
  onSelect: (item: T) => void;
  getLabel: (item: T) => string;
  getId: (item: T) => string;
  placeholder: string;
  icon: Component<{ class?: string }>;
  searchable?: boolean;
}

function Dropdown<T>(props: DropdownProps<T>) {
  const [isOpen, setIsOpen] = createSignal(false);
  const [searchQuery, setSearchQuery] = createSignal('');
  let dropdownRef: HTMLDivElement | undefined;
  let inputRef: HTMLInputElement | undefined;

  const filteredItems = () => {
    const query = searchQuery().toLowerCase();
    if (!query) return props.items;
    return props.items.filter(item => 
      props.getLabel(item).toLowerCase().includes(query)
    );
  };

  const handleOpen = (e: MouseEvent) => {
    e.stopPropagation();
    const willOpen = !isOpen();
    setIsOpen(willOpen);
    if (!willOpen) {
      setSearchQuery('');
    } else if (props.searchable) {
      setTimeout(() => inputRef?.focus(), 0);
    }
  };

  const handleSelect = (item: T, e: MouseEvent) => {
    e.stopPropagation();
    props.onSelect(item);
    setIsOpen(false);
    setSearchQuery('');
  };

  createEffect(() => {
    if (!isOpen()) return;
    
    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef && !dropdownRef.contains(e.target as Node)) {
        setIsOpen(false);
        setSearchQuery('');
      }
    };
    
    document.addEventListener('mousedown', handleClickOutside);
    onCleanup(() => document.removeEventListener('mousedown', handleClickOutside));
  });

  return (
    <div ref={dropdownRef} class="relative">
      <button
        onClick={(e) => handleOpen(e)}
        class="flex items-center gap-1.5 px-2 py-1 rounded hover:bg-neutral-700/50 transition-colors text-sm font-medium text-neutral-200"
      >
        <props.icon class="w-4 h-4 text-neutral-400" />
        <span class="max-w-[140px] truncate">
          {props.selected ? props.getLabel(props.selected) : props.placeholder}
        </span>
        <ChevronDownIcon class="w-3 h-3 text-neutral-500" />
      </button>

      <Show when={isOpen()}>
        <div class="absolute top-full left-0 mt-1 w-64 bg-neutral-800 border border-neutral-700 rounded-lg shadow-xl z-50 overflow-hidden">
          <Show when={props.searchable}>
            <div class="p-2 border-b border-neutral-700">
              <div class="flex items-center gap-2 px-2 py-1.5 bg-neutral-900 rounded border border-neutral-700 focus-within:border-neutral-500">
                <SearchIcon class="w-4 h-4 text-neutral-500" />
                <input
                  ref={inputRef}
                  type="text"
                  placeholder="Search..."
                  value={searchQuery()}
                  onInput={(e) => setSearchQuery(e.currentTarget.value)}
                  class="flex-1 bg-transparent text-sm text-neutral-200 placeholder-neutral-500 outline-none"
                />
              </div>
            </div>
          </Show>

          <div class="max-h-64 overflow-y-auto">
            <Show when={filteredItems().length === 0}>
              <div class="px-3 py-4 text-sm text-neutral-500 text-center">
                No items found
              </div>
            </Show>
            <For each={filteredItems()}>
              {(item) => (
                <button
                  onClick={(e) => handleSelect(item, e)}
                  class={`w-full px-3 py-2 text-left text-sm hover:bg-neutral-700/50 transition-colors flex items-center gap-2 ${
                    props.selected && props.getId(props.selected) === props.getId(item)
                      ? 'bg-neutral-700/30 text-white'
                      : 'text-neutral-300'
                  }`}
                >
                  <props.icon class="w-4 h-4 text-neutral-500" />
                  <span class="truncate">{props.getLabel(item)}</span>
                </button>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
}

export const BreadcrumbNav: Component = () => {
  const projectCtx = useProject();
  const sessionCtx = useSession();

  const handleProjectSelect = async (project: { path: string; name: string; kilns?: string[] }) => {
    await projectCtx.selectProject(project.path);
    const kiln = project.kilns?.[0] ?? project.path;
    await sessionCtx.refreshSessions({ kiln, workspace: project.path });
  };

  const handleSessionSelect = async (session: { id: string }) => {
    await sessionCtx.selectSession(session.id);
  };

  const handleNewSession = async () => {
    const project = projectCtx.currentProject();
    if (!project) return;

    const kiln = project.kilns[0] ?? project.path;
    await sessionCtx.createSession({
      kiln,
      session_type: 'chat',
    });
  };

  const sessionLabel = (session: { title: string | null; id: string }) => {
    return session.title ?? `Session ${session.id?.slice(0, 8) ?? 'unknown'}`;
  };

  return (
    <nav class="h-10 bg-neutral-900 border-b border-neutral-800 flex items-center px-3 gap-1 shrink-0">
      <Dropdown
        items={projectCtx.projects()}
        selected={projectCtx.currentProject()}
        onSelect={handleProjectSelect}
        getLabel={(p) => p.name}
        getId={(p) => p.path}
        placeholder="Select Project"
        icon={FolderIcon}
      />

      <SlashIcon />

      <Dropdown
        items={sessionCtx.sessions()}
        selected={sessionCtx.currentSession()}
        onSelect={handleSessionSelect}
        getLabel={sessionLabel}
        getId={(s) => s.id}
        placeholder="Select Session"
        icon={ChatBubbleIcon}
        searchable
      />

      <button
        onClick={handleNewSession}
        disabled={!projectCtx.currentProject()}
        class="ml-2 flex items-center gap-1 px-2 py-1 rounded text-sm font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-white border border-neutral-700"
        title="New Session"
      >
        <PlusIcon class="w-4 h-4" />
        <span class="hidden sm:inline">New</span>
      </button>

      <div class="flex-1" />

      <Show when={projectCtx.isLoading() || sessionCtx.isLoading()}>
        <div class="w-4 h-4 border-2 border-neutral-600 border-t-neutral-300 rounded-full animate-spin" />
      </Show>

      <Show when={projectCtx.error() || sessionCtx.error()}>
        <span class="text-xs text-red-400 truncate max-w-[200px]" title={projectCtx.error() ?? sessionCtx.error() ?? ''}>
          {projectCtx.error() ?? sessionCtx.error()}
        </span>
      </Show>
    </nav>
  );
};
