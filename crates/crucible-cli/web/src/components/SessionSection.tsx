import { Component, For, Show, createMemo } from 'solid-js';
import type { Session, KilnInfo } from '@/lib/types';
import { Search, X, Plus, Archive, Trash2 } from '@/lib/icons';

const KilnSelector: Component<{
  kilns: KilnInfo[];
  selected: string;
  onSelect: (kiln: string) => void;
}> = (props) => {
  const getKilnDisplayName = (kiln: KilnInfo) => {
    if (kiln.name) {
      return kiln.name;
    }
    const parts = kiln.path.split('/');
    return parts[parts.length - 1] || kiln.path;
  };

  return (
    <Show when={props.kilns.length > 0}>
      <div class="px-3 py-2 border-b border-neutral-800">
        <label class="text-xs text-neutral-500 block mb-1">Kiln</label>
        <Show
          when={props.kilns.length > 1}
          fallback={
            <div class="text-sm text-neutral-300 truncate" title={props.kilns[0].path}>
              {getKilnDisplayName(props.kilns[0])}
            </div>
          }
        >
          <select
            value={props.selected}
            onChange={(e) => props.onSelect(e.currentTarget.value)}
            class="w-full bg-surface-elevated text-neutral-200 text-sm px-2 py-1.5 rounded border border-neutral-700 focus:border-primary focus:outline-none"
          >
            <For each={props.kilns}>
              {(kiln) => (
                <option value={kiln.path}>{getKilnDisplayName(kiln)}</option>
              )}
            </For>
          </select>
        </Show>
      </div>
    </Show>
  );
};

export const StateIndicator: Component<{ state: Session['state'] }> = (props) => {
  const colorClass = () => {
    switch (props.state) {
      case 'active': return 'bg-green-500';
      case 'paused': return 'bg-yellow-500';
      case 'compacting': return 'bg-blue-500';
      case 'ended': return 'bg-neutral-500';
      default: return 'bg-neutral-500';
    }
  };

  return (
    <span class={`inline-block w-2 h-2 rounded-full ${colorClass()}`} title={props.state} />
  );
};

const SessionItem: Component<{
  session: Session;
  selected: boolean;
  onSelect: () => void;
  onArchive?: () => void;
  onDelete?: () => void;
}> = (props) => {
   return (
     <div
       onClick={props.onSelect}
      class={`group relative w-full text-left px-3 py-2 rounded-lg transition-colors cursor-pointer ${
        props.selected
          ? 'bg-primary/20 text-primary-hover'
          : 'hover:bg-surface-elevated text-neutral-300'
      }`}
       data-testid={`session-item-${props.session.id}`}
     >
      <div class="flex items-center gap-2">
        <StateIndicator state={props.session.state} />
        <span class="font-medium truncate flex-1">
          {props.session.title || `Session ${props.session.id?.slice(0, 8) ?? 'unknown'}`}
        </span>
      </div>
      <div class="text-xs text-neutral-500 mt-1">
        {props.session.agent_model || 'No model'}
      </div>

      {/* Action buttons — visible on hover */}
      <div class="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
        <Show when={props.onArchive}>
          <button
            type="button"
            class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
            title="Archive session"
            onClick={(e) => { e.stopPropagation(); props.onArchive?.(); }}
          >
            <Archive size={14} />
          </button>
        </Show>
        <Show when={props.onDelete}>
          <button
            type="button"
            class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
            title="Delete session"
            onClick={(e) => { e.stopPropagation(); props.onDelete?.(); }}
          >
            <Trash2 size={14} />
          </button>
        </Show>
      </div>
    </div>
  );
};

export const SessionSection: Component<{
  kilns: KilnInfo[];
  selectedKiln: string;
  onKilnSelect: (kiln: string) => void;
  sessionFilter: 'active' | 'all' | 'archived';
  onSessionFilterChange: (filter: 'active' | 'all' | 'archived') => void;
  searchQuery: string;
  isSearching: boolean;
  onSearchInput: (value: string) => void;
  onClearSearch: () => void;
  setSearchInputRef: (el: HTMLInputElement) => void;
  displayedSessions: Session[];
  currentSession: Session | undefined;
  onSelectSession: (id: string) => void;
  onArchiveSession: (id: string) => void;
  onDeleteSession: (id: string) => void;
  onCreateSession: () => void;
  isLoading: boolean;
  hasProviders: boolean;
}> = (props) => {
  const isDisabled = createMemo(() => props.isLoading || !props.selectedKiln || !props.hasProviders);

  return (
    <div class="border-t border-neutral-800">
      <KilnSelector
        kilns={props.kilns}
        selected={props.selectedKiln}
        onSelect={props.onKilnSelect}
      />

      <div class="p-3 flex items-center justify-between">
        <h2 class="text-sm font-semibold text-neutral-400 uppercase tracking-wide">Sessions</h2>
        <select
          data-testid="session-filter-dropdown"
          value={props.sessionFilter}
          onChange={(e) => props.onSessionFilterChange(e.target.value as 'active' | 'all' | 'archived')}
          class="text-xs bg-neutral-800 text-neutral-300 border border-neutral-700 rounded px-1 py-0.5"
        >
          <option value="active">Active</option>
          <option value="all">All</option>
          <option value="archived">Archived</option>
        </select>
      </div>

      <div class="px-3 pb-2">
        <div class="relative">
          <Search class="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-neutral-500" />
          <input
            ref={props.setSearchInputRef}
            type="text"
            value={props.searchQuery}
            onInput={(e) => props.onSearchInput(e.currentTarget.value)}
            placeholder="Search sessions..."
            class="w-full bg-neutral-800 text-neutral-200 text-sm pl-8 pr-7 py-1.5 rounded border border-neutral-700 focus:border-primary focus:outline-none placeholder:text-neutral-500"
            data-testid="session-search-input"
          />
          <Show when={props.searchQuery}>
            <button
              onClick={props.onClearSearch}
              class="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-neutral-500 hover:text-neutral-300 rounded"
            >
              <X class="w-3 h-3" />
            </button>
          </Show>
        </div>
      </div>

      <div class="p-2" data-testid="session-list">
        <Show when={props.isSearching}>
          <p class="text-neutral-500 text-sm text-center py-2">Searching...</p>
        </Show>

        <button
          onClick={props.onCreateSession}
          disabled={isDisabled()}
          class="w-full mt-2 px-3 py-2 text-sm text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
          data-testid="new-session-button"
        >
          <Plus class="w-3.5 h-3.5" />
          New Session
        </button>
        <Show when={!props.hasProviders}>
          <p class="text-xs text-neutral-500 text-center mt-1">No LLM providers detected</p>
        </Show>

        <For each={props.displayedSessions}>
          {(s) => (
            <SessionItem
              session={s}
              selected={props.currentSession?.id === s.id}
              onSelect={() => props.onSelectSession(s.id)}
              onArchive={() => props.onArchiveSession(s.id)}
              onDelete={() => props.onDeleteSession(s.id)}
            />
          )}
        </For>

        <Show when={!props.isSearching && props.displayedSessions.length === 0}>
          <Show
            when={props.searchQuery.trim()}
            fallback={<p class="text-neutral-500 text-sm text-center py-4">No sessions</p>}
          >
            <p class="text-neutral-500 text-sm text-center py-4">No sessions match "{props.searchQuery}"</p>
          </Show>
        </Show>

      </div>
    </div>
  );
};
