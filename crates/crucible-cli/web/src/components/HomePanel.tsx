import { Component, For, Show, createMemo, createResource } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { attentionStore } from '@/stores/attentionStore';
import { statusBarStore, pathBasename } from '@/stores/statusBarStore';
import { shellActions } from '@/stores/shellStore';
import { listNotes } from '@/lib/api';
import { openFileInEditor } from '@/lib/file-actions';
import { noteAbsolutePath } from '@/lib/note-actions';
import { sortByRecency, sessionDisplayTitle } from '@/lib/session-display';
import type { Session } from '@/lib/types';

// ── Home — the landing surface ───────────────────────────────────────────
// Crucible Shell design turn 5: greeting + kiln stats, a needs-you strip
// that leads to the Inbox, quick actions, resume-a-session, recent notes,
// and the graph placeholder (opens the editor until the graph view ships).

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
    <div class="font-mono font-semibold text-[10px] tracking-[0.08em] text-muted-dark mb-2.5">
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
      .slice(0, 6)
  );

  const resumeSessions = createMemo(() =>
    sortByRecency(sessionCtx.sessions().filter((s) => !s.archived)).slice(0, 4)
  );

  const sessionSub = (session: Session) => {
    const att = attentionStore.get(session.id);
    if (att?.pendingInteraction) return 'waiting on you';
    if (att?.isStreaming) return 'streaming';
    return session.state === 'ended' ? 'ended · resumable' : session.state;
  };

  const sessionDot = (session: Session) => {
    const att = attentionStore.get(session.id);
    if (att?.pendingInteraction) return 'bg-attention animate-pulse';
    if (att?.isStreaming) return 'bg-ok animate-pulse';
    return session.state === 'active' ? 'bg-ok' : 'bg-muted-dark';
  };

  return (
    <div class="h-full overflow-y-auto text-shell-ink [background:radial-gradient(ellipse_at_50%_0%,#16151a_0%,#0e0d11_60%)]">
      <div class="flex items-baseline gap-2.5 px-7 pt-5 flex-wrap">
        <span class="text-xl font-bold">{greetingForHour(new Date().getHours())}</span>
        <Show when={kilnName()}>
          <span class="font-mono text-[10.5px] text-muted-dark">
            {kilnName()}
            <Show when={notes()}> · {notes()!.length} notes</Show>
          </span>
        </Show>
      </div>

      <div class="grid gap-3.5 p-7 pt-4 grid-cols-1 lg:grid-cols-3">
        <div class="lg:col-span-3 flex gap-2.5 flex-wrap">
          <Show
            when={badge() > 0}
            fallback={
              <div class="flex-1 min-w-[240px] flex items-center gap-2.5 bg-ok/5 border border-ok/30 rounded-[9px] px-4 py-2.5 text-ok text-[12.5px]">
                ✓ all clear — nothing waiting on you
              </div>
            }
          >
            <button
              type="button"
              onClick={() => shellActions.goInbox()}
              class="flex-1 min-w-[240px] flex items-center gap-2.5 bg-attention/5 border border-attention/40 rounded-[9px] px-4 py-2.5 cursor-pointer hover:bg-attention/10 transition-colors text-left"
            >
              <span class="w-2 h-2 rounded-full bg-attention animate-pulse flex-none" />
              <span class="text-[12.5px] font-semibold">{badge()} need you</span>
              <span class="ml-auto text-attention text-xs">open inbox →</span>
            </button>
          </Show>
          <button
            type="button"
            onClick={() => window.dispatchEvent(new CustomEvent('crucible:new-session'))}
            class="flex items-center gap-2 border border-primary/50 text-primary rounded-[9px] px-4 py-2.5 text-[12.5px] font-semibold cursor-pointer hover:bg-primary/10 transition-colors"
          >
            + new session
          </button>
          <button
            type="button"
            onClick={() => shellActions.goEdit()}
            class="flex items-center gap-2 border border-white/10 text-shell-body rounded-[9px] px-4 py-2.5 text-[12.5px] cursor-pointer hover:bg-surface-elevated transition-colors"
          >
            ✎ open editor
          </button>
        </div>

        <div class="bg-shell-panel border border-white/[0.07] rounded-[10px] px-4 py-3.5 overflow-y-auto">
          <CardLabel>RESUME</CardLabel>
          <Show
            when={resumeSessions().length > 0}
            fallback={<div class="text-muted-dark text-xs">No sessions yet.</div>}
          >
            <div class="flex flex-col gap-2">
              <For each={resumeSessions()}>
                {(session) => (
                  <button
                    type="button"
                    onClick={() => void sessionCtx.selectSession(session.id).catch(() => {})}
                    class="flex items-center gap-2 px-2.5 py-2 rounded-[7px] border border-white/5 cursor-pointer hover:border-primary/50 transition-colors text-left"
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

        <div class="bg-shell-panel border border-white/[0.07] rounded-[10px] px-4 py-3.5 overflow-y-auto">
          <CardLabel>RECENT NOTES</CardLabel>
          <Show
            when={recentNotes().length > 0}
            fallback={<div class="text-muted-dark text-xs">No notes in this kiln yet.</div>}
          >
            <div class="flex flex-col gap-0.5 text-[12.5px]">
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
                    <span class="font-mono text-[9.5px] text-muted-dark flex-none">
                      {relativeTime(note.updated_at)}
                    </span>
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>

        <div class="bg-shell-panel border border-white/[0.07] rounded-[10px] px-4 py-3.5 flex flex-col min-h-[180px]">
          <CardLabel>GRAPH</CardLabel>
          <button
            type="button"
            onClick={() => shellActions.goEdit()}
            title="Graph view is coming — opens the editor"
            class="flex-1 border border-dashed border-white/10 rounded-lg relative overflow-hidden cursor-pointer hover:border-primary/50 transition-colors"
          >
            <span class="absolute left-[28%] top-[30%] w-2.5 h-2.5 rounded-full bg-shell-ink" />
            <span class="absolute left-[55%] top-[22%] w-[7px] h-[7px] rounded-full bg-muted" />
            <span class="absolute left-[62%] top-[55%] w-[7px] h-[7px] rounded-full bg-muted" />
            <span class="absolute left-[40%] top-[65%] w-2 h-2 rounded-sm bg-primary shadow-[0_0_10px_rgba(224,101,58,0.6)]" />
            <span class="font-mono text-[10.5px] text-muted-dark absolute bottom-2.5 left-0 right-0 text-center">
              open →
            </span>
          </button>
        </div>
      </div>
    </div>
  );
};

export default HomePanel;
