import { Component, For, Show, createMemo } from 'solid-js';
import { sessionDisplayTitle } from '@/lib/session-display';
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
      <div class="px-3 py-2 border-b border-hairline">
        <label class="text-xs text-muted-dark block mb-1">Kiln</label>
        <Show
          when={props.kilns.length > 1}
          fallback={
            <div class="text-sm text-shell-body truncate" title={props.kilns[0].path}>
              {getKilnDisplayName(props.kilns[0])}
            </div>
          }
        >
          <select
            value={props.selected}
            onChange={(e) => props.onSelect(e.currentTarget.value)}
            class="w-full bg-surface-elevated text-shell-ink text-sm px-2 py-1.5 rounded border border-hairline focus:border-primary focus:outline-none"
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
      case 'active': return 'bg-ok';
      case 'paused': return 'bg-attention';
      case 'compacting': return 'bg-primary';
      case 'ended': return 'bg-muted-dark';
      default: return 'bg-muted-dark';
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
       role="button"
       tabindex="0"
       onKeyDown={(e) => {
         if (e.key === 'Enter' || e.key === ' ') {
           e.preventDefault();
           props.onSelect();
         }
       }}
      class={`group relative w-full text-left px-3 py-2 rounded-lg transition-colors cursor-pointer ${
        props.selected
          ? 'bg-primary/20 text-primary-hover'
          : 'hover:bg-surface-elevated text-shell-body'
      }`}
       data-testid={`session-item-${props.session.id}`}
     >
      {/* pr-12 reserves the action-button strip so hover buttons never
          overlay the title; explicit 13px so the row doesn't inherit the
          browser/system base size. */}
      <div class="flex items-center gap-2 pr-12">
        <StateIndicator state={props.session.state} />
        <span class="text-[13px] font-medium truncate flex-1">
          {sessionDisplayTitle(props.session)}
        </span>
      </div>
      <div class="text-[11px] text-muted-dark mt-0.5 pr-12 truncate">
        {props.session.agent_model || 'No model'}
      </div>

      {/* Action buttons — visible on hover, in the reserved strip */}
      <div class="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 [@media(hover:none)]:opacity-100 transition-opacity duration-150">
        <Show when={props.onArchive}>
          <button
            type="button"
            class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
            title="Archive session"
            onClick={(e) => { e.stopPropagation(); props.onArchive?.(); }}
          >
            <Archive size={14} />
          </button>
        </Show>
        <Show when={props.onDelete}>
          <button
            type="button"
            class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors"
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
  /** False while the first provider probe is in flight — suppresses the
   * "no providers" message so a slow probe doesn't read as an error. */
  providersLoaded: boolean;
}> = (props) => {
  const isDisabled = createMemo(
    () => props.isLoading || !props.selectedKiln || (props.providersLoaded && !props.hasProviders),
  );

  return (
    <div class="border-t border-hairline">
      <KilnSelector
        kilns={props.kilns}
        selected={props.selectedKiln}
        onSelect={props.onKilnSelect}
      />

      <div class="p-3 flex items-center justify-between">
        <h2 class="text-sm font-semibold text-muted uppercase tracking-wide">Sessions</h2>
        <select
          data-testid="session-filter-dropdown"
          value={props.sessionFilter}
          onChange={(e) => props.onSessionFilterChange(e.target.value as 'active' | 'all' | 'archived')}
          class="text-xs bg-surface-elevated text-shell-body border border-hairline rounded px-1 py-0.5"
        >
          <option value="active">Active</option>
          <option value="all">All</option>
          <option value="archived">Archived</option>
        </select>
      </div>

      <div class="px-3 pb-2">
        <div class="relative">
          <Search class="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-dark" />
          <input
            ref={props.setSearchInputRef}
            type="text"
            value={props.searchQuery}
            onInput={(e) => props.onSearchInput(e.currentTarget.value)}
            placeholder="Search sessions..."
            class="w-full bg-control text-shell-ink text-sm pl-8 pr-7 py-1.5 rounded border border-hairline focus:border-primary focus:outline-none placeholder:text-muted-dark"
            data-testid="session-search-input"
          />
          <Show when={props.searchQuery}>
            <button
              onClick={props.onClearSearch}
              class="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-muted-dark hover:text-shell-body rounded"
            >
              <X class="w-3 h-3" />
            </button>
          </Show>
        </div>
      </div>

      <div class="p-2" data-testid="session-list">
        <Show when={props.isSearching}>
          <p class="text-muted-dark text-sm text-center py-2">Searching...</p>
        </Show>

        <button
          onClick={props.onCreateSession}
          disabled={isDisabled()}
          class="w-full mt-2 px-3 py-2 text-sm text-muted hover:text-shell-ink hover:bg-hover-wash rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
          data-testid="new-session-button"
        >
          <Plus class="w-3.5 h-3.5" />
          New Session
        </button>
        <Show when={props.providersLoaded && !props.hasProviders}>
          <p class="text-xs text-muted-dark text-center mt-1">No LLM providers detected</p>
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
            fallback={<p class="text-muted-dark text-sm text-center py-4">No sessions</p>}
          >
            <p class="text-muted-dark text-sm text-center py-4">No sessions match "{props.searchQuery}"</p>
          </Show>
        </Show>

      </div>
    </div>
  );
};
