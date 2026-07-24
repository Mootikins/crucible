import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
} from 'solid-js';
import { PanelShell } from './PanelShell';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import {
  getConfig,
  grepSearch,
  listKilns,
  searchSessions,
  semanticSearch,
  type GrepHit,
  type SemanticHit,
} from '@/lib/api';
import type { KilnListEntry, Session } from '@/lib/types';
import { swrLocal } from '@/lib/local-cache';
import { openFileInEditor } from '@/lib/file-actions';
import { pathBasename } from '@/stores/statusBarStore';
import { kilnLabel } from '@/lib/kiln-label';
import { relativeTime } from '@/lib/format-time';
import { treeSectionHeader } from '@/components/tree/tree-style';
import { Search, FileText, FolderGit2, FlaskConical, ClipboardList, ChevronDown, Check, X } from '@/lib/icons';

const HIT_LIMIT = 60;
const SEMANTIC_LIMIT = 20;
const DEBOUNCE_MS = 220;

// ---- scope-aware search options --------------------------------------------
type ScopeKind = 'everywhere' | 'kiln' | 'project' | 'sessions';
type SScope = { kind: ScopeKind; name: string; path?: string };
/** Text = ripgrep (literal); Semantic = vector similarity over embedded notes. */
type SearchMode = 'text' | 'semantic';

const NOTE_OPS: [string, string][] = [
  ['path:', 'match note path'], ['file:', 'match note name'], ['tag:', 'search for tags'],
  ['line:', 'keywords on the same line'], ['section:', 'under a heading'],
];
const FILE_OPS: [string, string][] = [
  ['path:', 'match file path'], ['file:', 'match file name'], ['ext:', 'filter by extension'],
  ['/regex/', 'regular expression'], ['case:', 'case-sensitive'],
];
const SESSION_OPS: [string, string][] = [
  ['agent:', 'by agent'], ['model:', 'by model'], ['kiln:', 'by kiln'], ['after:', 'active after a date'],
];
function opsFor(k: ScopeKind): [string, string][] {
  if (k === 'kiln') return NOTE_OPS;
  if (k === 'project') return FILE_OPS;
  if (k === 'sessions') return SESSION_OPS;
  return [['path:', 'note/file path'], ['file:', 'note/file name'], ['tag:', 'note tags'], ['ext:', 'file extension'], ['agent:', 'session agent']];
}
const optionsHeader = (k: ScopeKind) =>
  k === 'kiln' ? 'Note search' : k === 'project' ? 'File search' : k === 'sessions' ? 'Session search' : 'Search everywhere';
const scopeIcon = (k: ScopeKind) => (k === 'everywhere' ? Search : k === 'project' ? FolderGit2 : k === 'sessions' ? ClipboardList : FlaskConical);

/** Split a line into [before, match, after] for <mark> highlighting. */
function highlightParts(hit: GrepHit): [string, string, string] {
  const s = Math.max(0, Math.min(hit.matchStart, hit.text.length));
  const e = Math.max(s, Math.min(hit.matchEnd, hit.text.length));
  return [hit.text.slice(0, s), hit.text.slice(s, e), hit.text.slice(e)];
}

const HitRow: Component<{ hit: GrepHit; onOpen: () => void }> = (props) => {
  const parts = createMemo(() => highlightParts(props.hit));
  return (
    <button
      type="button"
      onClick={props.onOpen}
      title={`${props.hit.relPath}:${props.hit.line}`}
      class="w-full text-left px-3 py-1.5 rounded hover:bg-hover-wash transition-colors"
      data-testid="search-hit"
    >
      <div class="flex items-center gap-1.5 min-w-0">
        <FileText class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
        <span class="text-xs text-shell-body truncate">{pathBasename(props.hit.relPath)}</span>
        <span class="text-[10px] text-muted-dark shrink-0">:{props.hit.line}</span>
        <span class="text-[10px] text-muted-dark truncate ml-auto pl-2">{props.hit.relPath}</span>
      </div>
      <div class="mt-0.5 pl-5 text-[11px] font-mono leading-snug text-muted whitespace-pre-wrap break-all line-clamp-2">
        {parts()[0]}
        <mark class="bg-primary/25 text-shell-ink rounded-sm">{parts()[1]}</mark>
        {parts()[2]}
      </div>
    </button>
  );
};

/** A semantic (vector) note hit: note name, kiln-relative path, similarity. */
const SemanticRow: Component<{ hit: SemanticHit; onOpen: () => void }> = (props) => (
  <button
    type="button"
    onClick={props.onOpen}
    title={props.hit.relPath}
    class="w-full text-left px-3 py-1.5 rounded hover:bg-hover-wash transition-colors flex items-center gap-1.5"
    data-testid="search-semantic-hit"
  >
    <FileText class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
    <span class="text-xs text-shell-body truncate">{pathBasename(props.hit.relPath)}</span>
    <span class="text-[10px] text-muted-dark truncate min-w-0">{props.hit.relPath}</span>
    <span
      class="ml-auto shrink-0 text-[10px] font-mono tabular-nums text-primary/90 bg-primary/10 rounded px-1"
      title="similarity"
    >
      {Math.round(props.hit.score * 100)}%
    </span>
  </button>
);

/**
 * Content search scoped by a picker: pick Everywhere / Sessions / a kiln / a
 * project (prefilled from the current session's kiln). The scope drives which
 * corpora are searched (kiln → its notes, project → its files, sessions always
 * integrate) and which operator hints show. A Text/Semantic mode toggle swaps
 * note search between ripgrep (literal) and vector similarity. One debounced
 * query fans out.
 */
export const SearchPanel: Component = () => {
  const { projects } = useProjectSafe();
  const { selectSession, currentSession } = useSessionSafe();

  const [kilnPath, setKilnPath] = createSignal('');
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [query, setQuery] = createSignal('');
  const [debounced, setDebounced] = createSignal('');
  const [scope, setScope] = createSignal<SScope>({ kind: 'everywhere', name: 'Everywhere' });
  const [scopeTouched, setScopeTouched] = createSignal(false);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [mode, setMode] = createSignal<SearchMode>('text');

  const [noteHits, setNoteHits] = createSignal<GrepHit[]>([]);
  const [fileHits, setFileHits] = createSignal<GrepHit[]>([]);
  const [semanticHits, setSemanticHits] = createSignal<SemanticHit[]>([]);
  const [sessionHits, setSessionHits] = createSignal<Session[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  let inputRef: HTMLInputElement | undefined;

  const primaryKiln = () => kilnPath();
  const mruProject = () => projects()[0]?.path ?? '';
  const kilnDisplay = (path: string) => kilnLabel(path, kilns().find((k) => k.path === path)?.name);

  const scopeOptions = createMemo<SScope[]>(() => [
    { kind: 'everywhere', name: 'Everywhere' },
    { kind: 'sessions', name: 'Sessions' },
    ...kilns().map((k) => ({ kind: 'kiln' as const, name: kilnDisplay(k.path), path: k.path })),
    ...projects().map((p) => ({ kind: 'project' as const, name: p.name || pathBasename(p.path) || p.path, path: p.path })),
  ]);

  onMount(() => {
    swrLocal('config', getConfig, (cfg) => cfg?.kiln_path && setKilnPath(cfg.kiln_path));
    swrLocal('kilns', listKilns, setKilns);
    queueMicrotask(() => inputRef?.focus());
    const onFocus = () => { inputRef?.focus(); inputRef?.select(); };
    window.addEventListener('crucible:focus-search', onFocus);
    onCleanup(() => window.removeEventListener('crucible:focus-search', onFocus));
  });

  // Prefill the scope from the current session's kiln (context), until the user
  // picks a scope themselves.
  createEffect(() => {
    if (scopeTouched()) return;
    const k = currentSession()?.kiln;
    if (k) setScope({ kind: 'kiln', name: kilnDisplay(k), path: k });
  });

  const pickScope = (s: SScope) => { setScopeTouched(true); setScope(s); setPickerOpen(false); };

  // Debounce.
  createEffect(on(query, (q) => {
    const t = setTimeout(() => setDebounced(q.trim()), DEBOUNCE_MS);
    onCleanup(() => clearTimeout(t));
  }));

  const showNotes = () => scope().kind === 'everywhere' || scope().kind === 'kiln';
  const showFiles = () => scope().kind === 'everywhere' || scope().kind === 'project';
  // Which roots to grep for the current scope.
  const noteRoot = () => (scope().kind === 'kiln' ? scope().path ?? '' : primaryKiln());
  const fileRoot = () => (scope().kind === 'project' ? scope().path ?? '' : mruProject());

  // Run searches on query/scope/mode change; per-run token drops stale responses.
  let runToken = 0;
  createEffect(on([debounced, scope, mode], ([q]) => {
    const token = ++runToken;
    if (!q) { setNoteHits([]); setFileHits([]); setSemanticHits([]); setSessionHits([]); setError(null); setBusy(false); return; }
    setBusy(true);
    setError(null);
    const guard = <T,>(fn: () => T) => (runToken === token ? fn() : undefined);

    const nRoot = noteRoot();
    const semantic = mode() === 'semantic';

    // Notes: semantic (vector) or text (grep). The other note bucket is cleared
    // so switching modes never leaves stale hits of the wrong kind on screen.
    let notes: Promise<unknown>;
    if (!(showNotes() && nRoot)) {
      notes = Promise.resolve(guard(() => { setNoteHits([]); setSemanticHits([]); }));
    } else if (semantic) {
      setNoteHits([]);
      notes = semanticSearch(nRoot, q, SEMANTIC_LIMIT)
        .then((r) => guard(() => setSemanticHits(r)))
        .catch(() => guard(() => setSemanticHits([])));
    } else {
      setSemanticHits([]);
      notes = grepSearch(nRoot, q, { glob: '*.md', limit: HIT_LIMIT })
        .then((r) => guard(() => setNoteHits(r.hits)))
        .catch(() => guard(() => setNoteHits([])));
    }

    // Files: grep only (semantic search is notes-only). Hidden in semantic mode.
    const fRoot = fileRoot();
    const files = !semantic && showFiles() && fRoot
      ? grepSearch(fRoot, q, { limit: HIT_LIMIT }).then((r) => guard(() => setFileHits(r.hits))).catch(() => guard(() => setFileHits([])))
      : Promise.resolve(guard(() => setFileHits([])));

    // Sessions always integrate; scope to the kiln when one is selected.
    const sessKiln = scope().kind === 'kiln' ? scope().path : undefined;
    const sessions = searchSessions(q, sessKiln, 30).then((r) => guard(() => setSessionHits(r))).catch(() => guard(() => setSessionHits([])));

    void Promise.allSettled([notes, files, sessions]).then(() => guard(() => setBusy(false)));
  }));

  const counts = createMemo(() => ({
    notes: noteHits().length,
    semantic: semanticHits().length,
    files: fileHits().length,
    sessions: sessionHits().length,
  }));
  const total = () => counts().notes + counts().semantic + counts().files + counts().sessions;

  return (
    <PanelShell class="overflow-hidden">
      <div class="p-3 border-b border-hairline shrink-0 flex flex-col gap-2">
        <div class="flex items-center gap-2 bg-surface-base border border-hairline-strong rounded-lg px-2.5 py-1.5 focus-within:border-primary transition-colors">
          <Search class="w-4 h-4 shrink-0 text-muted-dark" />
          <input
            ref={inputRef}
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => { if (e.key === 'Escape' && query()) { e.stopPropagation(); setQuery(''); } }}
            placeholder={`Search ${scope().name.toLowerCase()}…`}
            aria-label="Search content"
            class="flex-1 min-w-0 bg-transparent text-sm text-shell-ink placeholder-muted-dark outline-none"
            data-testid="search-input"
          />
          <Show when={query()}>
            <button type="button" onClick={() => { setQuery(''); inputRef?.focus(); }} aria-label="Clear search"
              class="p-0.5 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash">
              <X class="w-3.5 h-3.5" />
            </button>
          </Show>
        </div>

        {/* Scope picker — prefilled to context; narrows/broadens the search. */}
        <div class="relative flex items-center gap-1.5 text-[11px]" data-search-scope>
          <span class="text-muted-dark">in</span>
          <button
            type="button"
            onClick={() => setPickerOpen((o) => !o)}
            data-testid="search-scope"
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full border border-hairline hover:bg-hover-wash text-shell-body"
          >
            {(() => { const I = scopeIcon(scope().kind); return <I class="w-3 h-3 text-muted-dark" />; })()}
            <span class="max-w-[140px] truncate">{scope().name}</span>
            <ChevronDown class="w-3 h-3 text-muted-dark" />
          </button>
          <Show when={pickerOpen()}>
            <ScopeMenu
              options={scopeOptions()}
              current={scope()}
              onPick={pickScope}
              onClose={() => setPickerOpen(false)}
            />
          </Show>

          {/* Text vs Semantic note search. Semantic ranks notes by meaning
              (vector similarity over embeddings); Text is literal ripgrep. */}
          <div class="ml-auto inline-flex rounded-full border border-hairline overflow-hidden" role="group" aria-label="Search mode">
            <For each={['text', 'semantic'] as SearchMode[]}>
              {(m) => (
                <button
                  type="button"
                  onClick={() => setMode(m)}
                  aria-pressed={mode() === m}
                  data-testid={`search-mode-${m}`}
                  class={`px-2 py-0.5 capitalize transition-colors ${mode() === m ? 'bg-primary/15 text-shell-ink' : 'text-muted-dark hover:bg-hover-wash'}`}
                >
                  {m}
                </button>
              )}
            </For>
          </div>
        </div>
      </div>

      <div class="flex-1 overflow-y-auto py-1" data-testid="search-results">
        <Show when={error()}>
          <div class="mx-3 my-2 px-3 py-2 text-xs text-error bg-error/10 rounded border border-error/30">{error()}</div>
        </Show>

        {/* Empty state: contextual operator hints for the chosen scope. */}
        <Show when={!debounced()}>
          <div class="px-3 py-2">
            <div class="py-1 text-[11px] font-semibold text-muted-dark">{optionsHeader(scope().kind)}</div>
            <For each={opsFor(scope().kind)}>
              {([op, d]) => (
                <div class="py-1 text-[12px] flex gap-2"><span class="font-semibold font-mono text-shell-ink">{op}</span><span class="text-muted-dark">{d}</span></div>
              )}
            </For>
          </div>
        </Show>

        <Show when={debounced() && !busy() && total() === 0}>
          <div class="px-3 py-8 text-center text-muted-dark text-xs">No matches for “{debounced()}”.</div>
        </Show>

        <Show when={mode() === 'semantic' && showNotes() && semanticHits().length > 0}>
          <div class={treeSectionHeader}>Notes · semantic · {counts().semantic}</div>
          <For each={semanticHits()}>
            {(hit) => <SemanticRow hit={hit} onOpen={() => openFileInEditor(hit.path, pathBasename(hit.relPath) || undefined)} />}
          </For>
        </Show>

        <Show when={mode() === 'text' && showNotes() && noteHits().length > 0}>
          <div class={treeSectionHeader}>Notes · {counts().notes}</div>
          <For each={noteHits()}>
            {(hit) => <HitRow hit={hit} onOpen={() => openFileInEditor(hit.path, pathBasename(hit.relPath) || undefined)} />}
          </For>
        </Show>

        <Show when={showFiles() && fileHits().length > 0}>
          <div class={treeSectionHeader}>Files · {counts().files}</div>
          <For each={fileHits()}>
            {(hit) => <HitRow hit={hit} onOpen={() => openFileInEditor(hit.path, pathBasename(hit.relPath) || undefined)} />}
          </For>
        </Show>

        <Show when={sessionHits().length > 0}>
          <div class={treeSectionHeader}>Sessions · {counts().sessions}</div>
          <For each={sessionHits()}>
            {(s) => (
              <button type="button" onClick={() => selectSession(s.id)} title={s.title ?? 'Untitled session'}
                class="w-full text-left px-3 py-1.5 rounded hover:bg-hover-wash transition-colors flex items-center gap-1.5"
                data-testid="search-session-hit">
                <ClipboardList class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
                <span class="text-xs text-shell-body truncate">{s.title ?? 'Untitled session'}</span>
                <Show when={s.started_at}>
                  <span class="text-[10px] text-muted-dark shrink-0 ml-auto pl-2">{relativeTime(s.started_at!)}</span>
                </Show>
              </button>
            )}
          </For>
        </Show>
      </div>
    </PanelShell>
  );
};

const ScopeMenu: Component<{ options: SScope[]; current: SScope; onPick: (s: SScope) => void; onClose: () => void }> = (props) => {
  onMount(() => {
    const close = (e: MouseEvent) => { if (!(e.target as HTMLElement).closest('[data-search-scope]')) props.onClose(); };
    document.addEventListener('click', close);
    onCleanup(() => document.removeEventListener('click', close));
  });
  const isSel = (s: SScope) => s.kind === props.current.kind && s.path === props.current.path;
  const Row: Component<{ s: SScope }> = (r) => {
    const I = scopeIcon(r.s.kind);
    return (
      <button type="button" onClick={() => props.onPick(r.s)}
        class="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-shell-body hover:bg-hover-wash"
        data-testid={`search-scope-${r.s.kind}${r.s.path ? '-' + pathBasename(r.s.path) : ''}`}>
        <I class="w-3.5 h-3.5 shrink-0 text-muted-dark" /><span class="truncate">{r.s.name}</span>
        <Show when={isSel(r.s)}><Check class="w-3.5 h-3.5 text-primary ml-auto" /></Show>
      </button>
    );
  };
  return (
    <div class="absolute left-6 top-6 z-30 w-56 max-h-[320px] overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1">
      <For each={props.options.filter((s) => s.kind === 'everywhere' || s.kind === 'sessions')}>{(s) => <Row s={s} />}</For>
      <Show when={props.options.some((s) => s.kind === 'kiln')}>
        <div class="px-3 pt-1.5 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-dark">Kilns</div>
        <For each={props.options.filter((s) => s.kind === 'kiln')}>{(s) => <Row s={s} />}</For>
      </Show>
      <Show when={props.options.some((s) => s.kind === 'project')}>
        <div class="px-3 pt-1.5 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-dark">Projects</div>
        <For each={props.options.filter((s) => s.kind === 'project')}>{(s) => <Row s={s} />}</For>
      </Show>
    </div>
  );
};
