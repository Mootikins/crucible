import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js';
import { Portal } from 'solid-js/web';
import { PanelShell } from './PanelShell';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { GrepHit, SemanticHit } from '@/lib/api';
import type { KilnListEntry } from '@/lib/types';
import { useGrepSearch, useSearchSessions, useSemanticSearch } from '@/lib/query/search';
import { useKilns } from '@/lib/query/kilns';
import { useConfig } from '@/lib/query/config';
import { openFileInEditor } from '@/lib/file-actions';
import { pathBasename } from '@/stores/statusBarStore';
import { kilnLabel } from '@/lib/kiln-label';
import { placePopup, type PopupPlacement } from '@/lib/popup-placement';
import { treeSectionHeader } from '@/components/tree/tree-style';
import { Search, FileText, FolderGit2, FlaskConical, ClipboardList, ChevronDown, Check, X } from '@/lib/icons';
import { sessionDefaultKiln } from '@/lib/session-scope';

/** Scope menu width (was `w-56`) and the height at which its list scrolls. */
const SCOPE_MENU_WIDTH = 224;
const SCOPE_MENU_MAX_HEIGHT = 320;

// ---- scope-aware search options --------------------------------------------
type ScopeKind = 'everywhere' | 'kiln' | 'project' | 'sessions';

/**
 * A search scope, as a discriminated union so a kiln's NAME and its DIRECTORY
 * cannot be mistaken for each other.
 *
 * They were one optional `path?: string` field, and the two consumers wanted
 * opposite things out of it: `semanticSearch`/`grepSearch` take a kiln
 * directory, `searchSessions` takes a registry name. Whichever the field held,
 * one of them silently searched nothing — a kiln picked from the menu stored a
 * path and returned zero session hits, while a scope prefilled from the current
 * session stored a name and greped a relative directory that does not exist.
 *
 * A kiln scope now carries both, under names that say which is which, and the
 * other three carry neither — so a new consumer has to say which one it means.
 */
type SScope =
  | { kind: 'everywhere'; name: string }
  | { kind: 'sessions'; name: string }
  /** `kiln` is the registry name (session search); `path` is the directory (grep, vectors). */
  | { kind: 'kiln'; name: string; kiln: string; path: string }
  | { kind: 'project'; name: string; path: string };

/** Identity for selection highlighting, across variants with no `path`. */
const scopeKey = (s: SScope) =>
  s.kind === 'kiln' ? `kiln:${s.kiln}` : s.kind === 'project' ? `project:${s.path}` : s.kind;
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
        <span class="text-floor text-muted-dark shrink-0">:{props.hit.line}</span>
        <span class="text-floor text-muted-dark truncate ml-auto pl-2">{props.hit.relPath}</span>
      </div>
      <div class="mt-0.5 pl-5 text-floor font-mono leading-snug text-muted whitespace-pre-wrap break-all line-clamp-2">
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
    <span class="text-floor text-muted-dark truncate min-w-0">{props.hit.relPath}</span>
    <span
      class="ml-auto shrink-0 text-floor font-mono tabular-nums text-primary/90 bg-primary/10 rounded px-1"
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

  const configQuery = useConfig();
  const kilnPath = () => configQuery.data?.kiln_path ?? '';
  const kilnsQuery = useKilns();
  const kilns = () => kilnsQuery.data ?? [];
  const [query, setQuery] = createSignal('');
  const [scope, setScope] = createSignal<SScope>({ kind: 'everywhere', name: 'Everywhere' });
  const [scopeTouched, setScopeTouched] = createSignal(false);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [mode, setMode] = createSignal<SearchMode>('text');

  let inputRef: HTMLInputElement | undefined;
  /** The scope chip; the portaled menu is placed against its viewport rect. */
  let scopeChipRef: HTMLButtonElement | undefined;

  const primaryKiln = () => kilnPath();
  const mruProject = () => projects()[0]?.path ?? '';
  /** The `kiln.list` entry a registry NAME belongs to, or undefined. */
  const kilnByName = (name: string) => kilns().find((k) => k.name === name);
  /** The scope for one `kiln.list` entry: its name for sessions, its path for notes. */
  const kilnScope = (k: KilnListEntry): SScope => ({
    kind: 'kiln',
    name: kilnLabel(k.path, k.name),
    // An entry the daemon lists without a registry name cannot scope a session
    // search; `''` keeps it out of the query rather than sending a path.
    kiln: k.name ?? '',
    path: k.path,
  });

  const scopeOptions = createMemo<SScope[]>(() => [
    { kind: 'everywhere', name: 'Everywhere' },
    { kind: 'sessions', name: 'Sessions' },
    ...kilns().map(kilnScope),
    ...projects().map((p) => ({ kind: 'project' as const, name: p.name || pathBasename(p.path) || p.path, path: p.path })),
  ]);

  onMount(() => {
    queueMicrotask(() => inputRef?.focus());
    const onFocus = () => { inputRef?.focus(); inputRef?.select(); };
    window.addEventListener('crucible:focus-search', onFocus);
    onCleanup(() => window.removeEventListener('crucible:focus-search', onFocus));
  });

  // Prefill the scope from the current session's kiln (context), until the user
  // picks a scope themselves.
  // `sessionDefaultKiln` returns a registry NAME, so it has to be resolved back
  // to an entry before it can scope a note search. It used to be stored
  // straight into `.path`, which greped a relative directory named after the
  // kiln — i.e. nothing — on every session that had one.
  createEffect(() => {
    if (scopeTouched()) return;
    const s = currentSession();
    const name = s ? sessionDefaultKiln(s) : null;
    const entry = name ? kilnByName(name) : undefined;
    if (entry) setScope(kilnScope(entry));
  });

  const pickScope = (s: SScope) => { setScopeTouched(true); setScope(s); setPickerOpen(false); };

  const showNotes = () => scope().kind === 'everywhere' || scope().kind === 'kiln';
  const showFiles = () => scope().kind === 'everywhere' || scope().kind === 'project';
  // Which roots to grep for the current scope.
  const noteRoot = () => { const s = scope(); return s.kind === 'kiln' ? s.path : primaryKiln(); };
  const fileRoot = () => { const s = scope(); return s.kind === 'project' ? s.path : mruProject(); };

  /**
   * The four searches, each asked only under the scope and mode that wants it.
   *
   * A root of `null` is "this bucket does not apply here", which is how the
   * mode switch empties the other note bucket: an entry nobody asks for holds
   * nothing, so there are no hits of the wrong kind left on screen. The
   * debounce and the stale-answer guard both live in the hooks — the guard is
   * the KEY, which the panel used to carry as a `runToken`.
   */
  const semanticNotes = useSemanticSearch(
    () => (showNotes() && mode() === 'semantic' ? noteRoot() || null : null),
    query,
  );
  const grepNotes = useGrepSearch(
    () => (showNotes() && mode() === 'text' ? noteRoot() || null : null),
    query,
    { glob: '*.md' },
  );
  const grepFiles = useGrepSearch(
    () => (showFiles() && mode() === 'text' ? fileRoot() || null : null),
    query,
  );
  // The NAME. `searchSessions` takes registry names, and the route refuses a
  // set that names kilns and resolves none of them — a path here is a
  // guaranteed 422 rather than the silent empty result it used to be.
  const sessionSearch = useSearchSessions(query, () => {
    const s = scope();
    return s.kind === 'kiln' ? s.kiln || undefined : undefined;
  });

  const semanticHits = () => semanticNotes.data ?? [];
  const noteHits = (): GrepHit[] => grepNotes.data?.hits ?? [];
  const fileHits = (): GrepHit[] => grepFiles.data?.hits ?? [];
  /**
   * The transcript lines that matched, and the daemon's note when the search
   * was unscoped. A match names its session; the panel shows the line, because
   * the line is what the query found.
   */
  const sessionHits = () => sessionSearch.data?.matches ?? [];
  const sessionNote = () => sessionSearch.data?.note ?? null;

  const busy = () =>
    semanticNotes.isFetching ||
    grepNotes.isFetching ||
    grepFiles.isFetching ||
    sessionSearch.isFetching;

  /**
   * The first refusal, if any.
   *
   * The banner was here before and nothing ever filled it: every failure was
   * caught into an empty bucket, so a kiln with no embedding provider and a
   * kiln with no matches read the same. The daemon's own sentence goes in it.
   */
  const error = () =>
    [semanticNotes.error, grepNotes.error, grepFiles.error, sessionSearch.error].find(Boolean)
      ?.message ?? null;

  /** The query the results on screen answer, for the prompt and the empty state. */
  const asked = () => query().trim();

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
            class="focus-ring flex-1 min-w-0 bg-transparent text-sm text-shell-ink placeholder-muted-dark"
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
        {/* No `relative` — the menu is portaled and viewport-positioned. */}
        <div class="flex items-center gap-1.5 text-floor" data-search-scope>
          <span class="text-muted-dark">in</span>
          <button
            ref={scopeChipRef}
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
              anchor={() => scopeChipRef}
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
        <Show when={!asked()}>
          <div class="px-3 py-2">
            <div class="py-1 text-floor font-semibold text-muted-dark">{optionsHeader(scope().kind)}</div>
            <For each={opsFor(scope().kind)}>
              {([op, d]) => (
                <div class="py-1 text-reading flex gap-2"><span class="font-semibold font-mono text-shell-ink">{op}</span><span class="text-muted-dark">{d}</span></div>
              )}
            </For>
          </div>
        </Show>

        <Show when={asked() && !busy() && total() === 0}>
          <div class="px-3 py-8 text-center text-muted-dark text-xs">No matches for “{asked()}”.</div>
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

        {/* The daemon's own sentence for a search that scoped to no kiln and
            therefore searched nothing. "No results" would be a lie there. */}
        <Show when={sessionNote()}>
          <div class="px-3 py-1.5 text-floor text-muted-dark" data-testid="search-session-note">
            {sessionNote()}
          </div>
        </Show>

        <Show when={sessionHits().length > 0}>
          <div class={treeSectionHeader}>Sessions · {counts().sessions}</div>
          <For each={sessionHits()}>
            {(match) => (
              <button type="button" onClick={() => selectSession(match.session_id)} title={match.session_id}
                class="w-full text-left px-3 py-1.5 rounded hover:bg-hover-wash transition-colors flex items-center gap-1.5"
                data-testid="search-session-hit">
                <ClipboardList class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
                <span class="text-xs text-shell-body truncate">{match.context}</span>
                <Show when={match.line > 0}>
                  <span class="text-floor text-muted-dark shrink-0 ml-auto pl-2">L{match.line}</span>
                </Show>
              </button>
            )}
          </For>
        </Show>
      </div>
    </PanelShell>
  );
};

/**
 * The scope picker's list.
 *
 * Portaled and viewport-positioned for the same reason `ChipSelect` is: the
 * panel renders inside an EdgePanel whose slide frame is `overflow-hidden` and
 * whose inner wrapper always carries a `translate` — a stacking context AND a
 * containing block, so an in-flow `absolute` menu is clipped at the panel's
 * right edge and painted under the center pane whatever z-index it asks for.
 */
const ScopeMenu: Component<{
  options: SScope[];
  current: SScope;
  /** The trigger chip — the menu is placed against its viewport rect. */
  anchor: () => HTMLElement | undefined;
  onPick: (s: SScope) => void;
  onClose: () => void;
}> = (props) => {
  const [pos, setPos] = createSignal<PopupPlacement | null>(null);
  let menuRef: HTMLDivElement | undefined;

  const place = () => {
    const el = props.anchor();
    if (!el) return;
    setPos(
      placePopup(el.getBoundingClientRect(), { width: window.innerWidth, height: window.innerHeight }, {
        width: SCOPE_MENU_WIDTH,
        preferredHeight: SCOPE_MENU_MAX_HEIGHT,
        gap: 4,
      }),
    );
  };

  onMount(() => {
    place();
    // Portaled out of `[data-search-scope]`, so containment no longer means
    // "inside the control" — the menu itself has to be checked too.
    const close = (e: MouseEvent) => {
      const t = e.target as Node;
      if ((t as HTMLElement).closest?.('[data-search-scope]')) return;
      if (menuRef?.contains(t)) return;
      props.onClose();
    };
    document.addEventListener('click', close);
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    onCleanup(() => {
      document.removeEventListener('click', close);
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
    });
  });
  const isSel = (s: SScope) => scopeKey(s) === scopeKey(props.current);
  const Row: Component<{ s: SScope }> = (r) => {
    const I = scopeIcon(r.s.kind);
    return (
      <button type="button" onClick={() => props.onPick(r.s)}
        class="w-full flex items-center gap-2 px-3 py-1.5 text-reading text-shell-body hover:bg-hover-wash"
        data-testid={`search-scope-${r.s.kind}${'path' in r.s && r.s.path ? '-' + pathBasename(r.s.path) : ''}`}>
        <I class="w-3.5 h-3.5 shrink-0 text-muted-dark" /><span class="truncate">{r.s.name}</span>
        <Show when={isSel(r.s)}><Check class="w-3.5 h-3.5 text-primary ml-auto" /></Show>
      </button>
    );
  };
  return (
    <Portal>
      <div
        ref={menuRef}
        data-testid="search-scope-menu"
        class="z-50 overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1"
        style={{
          position: 'fixed',
          left: `${pos()?.left ?? 0}px`,
          ...(pos()?.bottom !== undefined
            ? { bottom: `${pos()!.bottom}px` }
            : { top: `${pos()?.top ?? 0}px` }),
          width: `${SCOPE_MENU_WIDTH}px`,
          'max-height': `${pos()?.maxHeight ?? SCOPE_MENU_MAX_HEIGHT}px`,
        }}
      >
        <For each={props.options.filter((s) => s.kind === 'everywhere' || s.kind === 'sessions')}>{(s) => <Row s={s} />}</For>
        <Show when={props.options.some((s) => s.kind === 'kiln')}>
          <div class="px-3 pt-1.5 pb-1 text-floor font-semibold uppercase tracking-wide text-muted-dark">Kilns</div>
          <For each={props.options.filter((s) => s.kind === 'kiln')}>{(s) => <Row s={s} />}</For>
        </Show>
        <Show when={props.options.some((s) => s.kind === 'project')}>
          <div class="px-3 pt-1.5 pb-1 text-floor font-semibold uppercase tracking-wide text-muted-dark">Projects</div>
          <For each={props.options.filter((s) => s.kind === 'project')}>{(s) => <Row s={s} />}</For>
        </Show>
      </div>
    </Portal>
  );
};
