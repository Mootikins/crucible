import { Component, For, Show, createMemo, createResource, createSignal } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { attentionStore } from '@/stores/attentionStore';
import { statusBarStore, pathBasename } from '@/stores/statusBarStore';
import { shellActions } from '@/stores/shellStore';
import { openPanelTab } from '@/lib/panel-actions';
import { listNotes } from '@/lib/api';
import { openFileInEditor } from '@/lib/file-actions';
import { noteAbsolutePath } from '@/lib/note-actions';
import { openDraftSession, setDraftPrefill } from '@/lib/draft-session';
import { sortByRecency, sessionDisplayTitle } from '@/lib/session-display';
import { SECTION_LABEL_CLASS } from '@/components/ui/SectionLabel';
import { Pencil } from '@/lib/icons';
import type { Session } from '@/lib/types';

// ── Home — the landing surface ───────────────────────────────────────────
// The composer is the hero: a session starts by typing, the way Cursor opens
// with its prompt box. Everything else — the needs-you strip, resume, recent
// notes, the graph teaser — is a quiet supporting grid beneath it, so the page
// reads as a purposeful starting point rather than a scatter of small cards.

export function greetingForHour(hour: number): string {
  if (hour < 5) return 'Up late';
  if (hour < 12) return 'Good morning';
  if (hour < 18) return 'Good afternoon';
  return 'Good evening';
}

export function relativeTime(iso: string, now: number = Date.now()): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return '';
  const seconds = Math.max(0, Math.floor((now - then) / 1000));
  if (seconds < 60) return 'just now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function CardLabel(props: { children: string }) {
  return (
    <div class={SECTION_LABEL_CLASS}>
      {props.children}
    </div>
  );
}

export const HomePanel: Component = () => {
  const sessionCtx = useSessionSafe();
  const badge = attentionStore.attentionCount;
  const kilnName = () => pathBasename(statusBarStore.kilnPath());

  const [notes] = createResource(
    () => statusBarStore.kilnPath(),
    (kiln) => listNotes(kiln).catch(() => [])
  );

  const recentNotes = createMemo(() =>
    [...(notes() ?? [])]
      .sort((a, b) => Date.parse(b.updated_at || '') - Date.parse(a.updated_at || ''))
      .slice(0, 10)
  );

  // Most-recent handful only — a "jump back in" shortcut, not a second copy of
  // the sidebar's full session list.
  const resumeSessions = createMemo(() =>
    sortByRecency(sessionCtx.sessions().filter((s) => !s.archived)).slice(0, 3)
  );

  const [composer, setComposer] = createSignal('');
  let composerRef: HTMLTextAreaElement | undefined;

  const autoGrow = (el: HTMLTextAreaElement) => {
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 168)}px`;
  };

  // Hand the typed text to the draft surface (reviewed there, not auto-sent);
  // an empty composer just opens an empty draft.
  const startSession = () => {
    const text = composer().trim();
    if (text) setDraftPrefill(text);
    openDraftSession();
    setComposer('');
    if (composerRef) composerRef.style.height = 'auto';
  };

  const onComposerKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      startSession();
    }
  };

  const sessionSub = (session: Session) => {
    const att = attentionStore.get(session.id);
    if (att?.pendingInteraction) return 'waiting on you';
    if (att?.isStreaming) return 'streaming';
    const rel = relativeTime(session.last_activity ?? session.started_at ?? '');
    if (rel) return `last active ${rel}`;
    return session.state === 'ended' ? 'resumable' : session.state;
  };

  const sessionDot = (session: Session) => {
    const att = attentionStore.get(session.id);
    if (att?.pendingInteraction) return 'bg-attention animate-pulse';
    if (att?.isStreaming) return 'bg-ok animate-pulse';
    return session.state === 'active' ? 'bg-ok' : 'bg-muted-dark';
  };

  return (
    <div class="h-full overflow-y-auto text-shell-ink bg-shell-bg">
      <div class="mx-auto w-full max-w-3xl px-7 py-9 flex flex-col gap-6">
        {/* Greeting + a quiet editor shortcut. */}
        <div class="flex items-baseline gap-2.5 flex-wrap">
          <span class="text-xl font-bold">{greetingForHour(new Date().getHours())}</span>
          <Show when={kilnName()}>
            <span class="font-mono text-[10.5px] text-muted-dark">
              {kilnName()}
              <Show when={notes()}> · {notes()!.length} notes</Show>
            </span>
          </Show>
          <button
            type="button"
            onClick={() => shellActions.goEdit()}
            class="ml-auto flex items-center gap-1.5 text-[11px] text-muted hover:text-shell-ink transition-colors cursor-pointer"
          >
            <Pencil class="w-3 h-3" aria-hidden="true" /> Open editor
          </button>
        </div>

        {/* Needs-you strip — the one thing that should pull the eye if present. */}
        <Show
          when={badge() > 0}
          fallback={
            <div class="flex items-center gap-2.5 bg-ok/5 border border-ok/25 rounded-xl px-4 py-2.5 text-ok text-[12.5px]">
              <span class="w-1.5 h-1.5 rounded-full bg-ok flex-none" />
              All clear — nothing waiting on you
            </div>
          }
        >
          <button
            type="button"
            onClick={() => shellActions.goInbox()}
            class="flex items-center gap-2.5 bg-attention/5 border border-attention/40 rounded-xl px-4 py-2.5 cursor-pointer hover:bg-attention/10 transition-colors text-left"
          >
            <span class="w-2 h-2 rounded-full bg-attention animate-pulse flex-none" />
            <span class="text-[12.5px] font-semibold">{badge()} need you</span>
            <span class="ml-auto text-attention text-xs">open inbox →</span>
          </button>
        </Show>

        {/* HERO — the composer. Focus lifts the hairline to an ember glow; the
            single accented element on an otherwise quiet page. */}
        <div class="rounded-2xl border border-hairline bg-surface-elevated transition-colors focus-within:border-primary/70 focus-within:shadow-[0_0_0_3px] focus-within:shadow-primary/15">
          <textarea
            ref={composerRef}
            value={composer()}
            onInput={(e) => {
              setComposer(e.currentTarget.value);
              autoGrow(e.currentTarget);
            }}
            onKeyDown={onComposerKeyDown}
            rows={1}
            placeholder="Ask anything — starts a new session"
            aria-label="Start a session"
            class="w-full bg-transparent resize-none outline-none text-shell-ink placeholder-muted-dark text-[15px] leading-relaxed px-4 pt-3.5 pb-1"
            data-testid="home-composer"
          />
          <div class="flex items-center gap-2 px-3 pb-2.5 pt-1">
            <span class="font-mono text-[10px] text-muted-dark select-none">
              ↵ to start · ⇧↵ newline
            </span>
            <button
              type="button"
              onClick={startSession}
              aria-label="Start session"
              class="ml-auto flex items-center justify-center w-8 h-8 rounded-lg bg-primary text-white hover:bg-primary-hover transition-colors cursor-pointer"
            >
              <svg viewBox="0 0 24 24" fill="none" class="w-4 h-4" aria-hidden="true">
                <path
                  d="M12 19V5M12 5l-6 6M12 5l6 6"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          </div>
        </div>

        {/* Supporting grid: left column stacks Resume + Graph to roughly match
            the taller Recent-notes column, so neither leaves a void. */}
        <div class="grid gap-3.5 md:grid-cols-2 items-start">
          <div class="flex flex-col gap-3.5">
            <div class="bg-shell-panel border border-hairline rounded-2xl px-4 py-3.5">
              <CardLabel>RESUME</CardLabel>
              <Show
                when={resumeSessions().length > 0}
                fallback={
                  <div class="text-muted-dark text-xs mt-2.5">
                    No sessions yet — start one above.
                  </div>
                }
              >
                <div class="flex flex-col gap-1.5 mt-2.5">
                  <For each={resumeSessions()}>
                    {(session) => (
                      <button
                        type="button"
                        onClick={() => void sessionCtx.selectSession(session.id).catch(() => {})}
                        class="flex items-center gap-2.5 px-2.5 py-2 rounded-lg border border-hairline cursor-pointer hover:border-primary/50 transition-colors text-left"
                      >
                        <span class={`w-[7px] h-[7px] rounded-full flex-none ${sessionDot(session)}`} />
                        <span class="flex-1 min-w-0">
                          <span class="block text-[12.5px] font-semibold truncate">
                            {sessionDisplayTitle(session)}
                          </span>
                          <span class="block text-[10.5px] text-muted-dark truncate">
                            {sessionSub(session)}
                          </span>
                        </span>
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </div>

            <button
              type="button"
              onClick={() => openPanelTab('graph')}
              title="Open the knowledge graph"
              class="group bg-shell-panel border border-hairline rounded-2xl px-4 py-3.5 text-left cursor-pointer hover:border-primary/50 transition-colors flex flex-col"
            >
              <div class="flex items-center justify-between">
                <CardLabel>GRAPH</CardLabel>
                <span class="font-mono text-[10px] text-muted-dark group-hover:text-primary transition-colors">
                  open →
                </span>
              </div>
              <svg
                viewBox="0 0 200 84"
                class="w-full h-[84px] mt-2.5"
                aria-hidden="true"
                preserveAspectRatio="xMidYMid meet"
              >
                <g class="stroke-hairline-strong" stroke-width="1">
                  <line x1="40" y1="24" x2="92" y2="16" />
                  <line x1="92" y1="16" x2="150" y2="28" />
                  <line x1="40" y1="24" x2="62" y2="58" />
                  <line x1="62" y1="58" x2="118" y2="62" />
                  <line x1="92" y1="16" x2="118" y2="62" />
                  <line x1="118" y1="62" x2="168" y2="56" />
                  <line x1="150" y1="28" x2="168" y2="56" />
                </g>
                <circle cx="40" cy="24" r="3.5" class="fill-muted" />
                <circle cx="92" cy="16" r="3.5" class="fill-muted" />
                <circle cx="150" cy="28" r="3.5" class="fill-muted" />
                <circle cx="62" cy="58" r="3.5" class="fill-muted" />
                <circle cx="168" cy="56" r="3.5" class="fill-muted" />
                <circle cx="118" cy="62" r="9" class="fill-primary/20" />
                <circle cx="118" cy="62" r="4.5" class="fill-primary" />
              </svg>
            </button>
          </div>

          <div class="bg-shell-panel border border-hairline rounded-2xl px-4 py-3.5">
            <CardLabel>RECENT NOTES</CardLabel>
            <Show
              when={recentNotes().length > 0}
              fallback={
                <div class="text-muted-dark text-xs mt-2.5">
                  {notes.loading ? 'Loading notes…' : 'No notes in this kiln yet.'}
                </div>
              }
            >
              <div class="flex flex-col gap-0.5 text-[12.5px] mt-2">
                <For each={recentNotes()}>
                  {(note) => (
                    <button
                      type="button"
                      onClick={() =>
                        // Note records carry kiln-relative paths; the file API
                        // addresses files absolutely.
                        openFileInEditor(
                          noteAbsolutePath(note.path, statusBarStore.kilnPath() ?? ''),
                          note.name,
                        )
                      }
                      class="flex justify-between gap-2 px-2 py-1.5 rounded-md cursor-pointer hover:bg-surface-elevated transition-colors text-left"
                    >
                      <span class="truncate">{note.title || note.name}</span>
                      <span class="font-mono text-[10px] text-muted-dark flex-none">
                        {relativeTime(note.updated_at)}
                      </span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
};

export default HomePanel;
