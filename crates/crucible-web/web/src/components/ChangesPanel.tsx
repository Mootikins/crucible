/**
 * Changes — the session record of the current session, and the writes that
 * need a merge.
 *
 * Roots → files over the session record diffset (`session_base` → disk). A
 * file opens the session record in the diff pane, focused on that file. No
 * change has a decision:
 * the daemon keeps the attribution record, and the diff pane shows the text.
 *
 * Renders in the RIGHT edge region alongside Activity and Backlinks, which
 * puts it OUTSIDE the per-chat-tab `ChatProvider` — hence `useSessionSafe`
 * plus the global review store rather than `useChatSafe`, which would silently
 * hand back the inert fallback context.
 */
import { Component, For, Show, createMemo, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import { notificationActions } from '@/stores/notificationStore';
import { reviewActions, reviewStore, useReviewSession } from '@/lib/review-store';
import { hit } from '@/lib/touch';
import { conflictActions, conflictStore, openConflict } from '@/lib/conflicts';
import type { DiffFileEntry } from '@/lib/diffset';
import { AlertTriangle, Check, RefreshCw } from '@/lib/icons';
import { useProposals } from '@/lib/query/proposals';
import { authorLabel } from '@/lib/proposal-api';
import { openDiff } from '@/lib/panel-actions';

/** Roots in first-seen order, each with its files. */
function groupByRoot(files: DiffFileEntry[]): { root: string; files: DiffFileEntry[] }[] {
  const roots: { root: string; files: DiffFileEntry[] }[] = [];
  for (const file of files) {
    const existing = roots.find((r) => r.root === file.root);
    if (existing) existing.files.push(file);
    else roots.push({ root: file.root, files: [file] });
  }
  return roots;
}

export const ChangesPanel: Component = () => {
  const { currentSession } = useSessionSafe();
  const sessionId = () => currentSession()?.session_id;
  useReviewSession(sessionId);

  const state = () => reviewStore.session(sessionId());
  const roots = createMemo(() => groupByRoot(state().files));

  const openComments = createMemo(() => state().comments.filter((c) => !c.resolved));

  // A conflict is a disposition of a note write, and this panel is where a
  // disposition is made. Read once on mount: the drain is what creates one,
  // and the drain is not this panel's event stream.
  onMount(() => void conflictActions.refresh().catch(() => undefined));
  const conflicts = () => conflictStore.list();
  // A stale or conflicted proposal needs a merge, as a conflict does. An open
  // proposal waits in the Inbox only.
  const proposalQuery = useProposals();
  const mergeProposals = createMemo(() =>
    (proposalQuery.data ?? []).filter(
      (p) => p.state.kind === 'stale' || p.state.kind === 'conflicted',
    ),
  );

  return (
    <PanelShell>
      <PanelHeader title="Changes" class="shrink-0">
        <div class="mt-1.5 flex items-center gap-2">
          <span class="text-floor text-muted-dark" data-testid="changes-count">
            {state().files.length} {state().files.length === 1 ? 'file' : 'files'}
          </span>
          <button
            type="button"
            title="Refresh"
            data-testid="changes-refresh"
            disabled={!sessionId() || state().loading}
            onClick={() => {
              const id = sessionId();
              if (id) void reviewActions.refresh(id);
            }}
            class="ml-auto rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50"
          >
            <RefreshCw class={`w-3.5 h-3.5 ${state().loading ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </PanelHeader>

      <div class="flex-1 overflow-y-auto">
        {/* Above the roots, and OUTSIDE the session check. A conflict
            belongs to no session's record, and on a desktop there is no
            offline badge to carry it — so a conflict that only showed under a
            selected session would be a write nothing lists. The count above
            stays about the session. */}
        <Show when={conflicts().length > 0}>
          <div data-testid="changes-conflicts">
            <div class="flex items-center gap-1 px-3 py-1 text-floor uppercase tracking-wider text-attention bg-attention/10 border-b border-hairline">
              <AlertTriangle class="w-3 h-3 shrink-0" />
              Conflicts
            </div>
            <For each={conflicts()}>
              {(row) => (
                <div
                  class="flex items-center gap-2 border-b border-hairline px-3 py-1.5"
                  data-testid={`changes-conflict-${row.path}`}
                >
                  <span
                    class="min-w-0 flex-1 truncate text-xs font-mono text-shell-ink"
                    title={row.path}
                  >
                    {row.path}
                  </span>
                  <span class="shrink-0 text-floor text-muted-dark">{row.regions.length}</span>
                  <button
                    type="button"
                    title={`Settle ${row.path}`}
                    data-testid={`changes-conflict-open-${row.path}`}
                    onClick={() => openConflict(row.path)}
                    class={`shrink-0 rounded border border-hairline px-2 py-0.5 text-floor text-shell-ink hover:bg-hover-wash ${hit()}`}
                  >
                    Open
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>

        {/* Beside the conflicts, and outside the session check for the same
            reason: a proposal belongs to no session. */}
        <Show when={mergeProposals().length > 0}>
          <div data-testid="changes-proposals">
            <div class="flex items-center gap-1 px-3 py-1 text-floor uppercase tracking-wider text-attention bg-attention/10 border-b border-hairline">
              <AlertTriangle class="w-3 h-3 shrink-0" />
              Proposals
            </div>
            <For each={mergeProposals()}>
              {(proposal) => (
                <div
                  class="flex items-center gap-2 border-b border-hairline px-3 py-1.5"
                  data-testid={`changes-proposal-${proposal.id}`}
                >
                  <span
                    class="min-w-0 flex-1 truncate text-xs text-shell-ink"
                    title={`${authorLabel(proposal)}: ${proposal.title}`}
                  >
                    {proposal.title}
                  </span>
                  <span
                    class="shrink-0 text-floor text-muted-dark"
                    data-testid={`changes-proposal-state-${proposal.id}`}
                  >
                    {proposal.state.kind}
                  </span>
                  <button
                    type="button"
                    title={`Review ${proposal.title}`}
                    data-testid={`changes-proposal-open-${proposal.id}`}
                    onClick={() => openDiff({ kind: 'proposal', id: proposal.id })}
                    class={`shrink-0 rounded border border-hairline px-2 py-0.5 text-floor text-shell-ink hover:bg-hover-wash ${hit()}`}
                  >
                    Open
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>

        <Show
          when={sessionId()}
          fallback={<p class="p-3 text-xs text-muted-dark">No session selected.</p>}
        >
          <Show when={state().error}>
            <p class="px-3 py-2 text-xs text-error" data-testid="changes-error">
              {state().error}
            </p>
          </Show>

          <Show when={state().loaded && state().files.length === 0}>
            <p class="p-3 text-xs text-muted-dark" data-testid="changes-empty">
              No changes in this session yet.
            </p>
          </Show>

          <For each={roots()}>
            {(root) => (
              <div>
                <div
                  class="px-3 py-1 text-floor uppercase tracking-wider text-muted-dark bg-surface-base border-b border-hairline truncate"
                  title={root.root}
                >
                  {root.root.split('/').filter(Boolean).pop() ?? root.root}
                </div>
                <For each={root.files}>
                  {(file) => (
                    <button
                      type="button"
                      data-testid={`changes-file-${file.path}`}
                      title={`Open the session record at ${file.path}`}
                      onClick={() => {
                        const id = sessionId();
                        if (id) openDiff({ kind: 'session_record', session: id }, { root: file.root, path: file.path });
                      }}
                      class="w-full min-w-0 flex items-center gap-2 border-b border-hairline px-3 py-1.5 text-left hover:bg-hover-wash"
                    >
                      <span class="flex-1 min-w-0 truncate text-xs font-mono text-shell-ink">
                        {file.path}
                      </span>
                      <span class="shrink-0 text-floor text-muted-dark">{file.status.kind}</span>
                      <Show when={!file.binary && !file.too_large}>
                        <span class="shrink-0 text-floor font-mono text-ok">+{file.added}</span>
                        <span class="shrink-0 text-floor font-mono text-error">-{file.removed}</span>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            )}
          </For>

          <Show when={openComments().length > 0}>
            <div class="border-t border-hairline mt-2">
              <div class="px-3 py-1 text-floor uppercase tracking-wider text-muted-dark">
                Comments
              </div>
              <For each={openComments()}>
                {(comment) => (
                  <div
                    class="px-3 py-1.5 flex items-start gap-2"
                    data-testid={`comment-${comment.id}`}
                  >
                    <div class="flex-1 min-w-0">
                      <div class="text-floor text-muted-dark font-mono truncate">
                        {comment.path}:{comment.line_range.start} · {comment.author}
                      </div>
                      <div class="text-xs text-shell-body break-words">{comment.body}</div>
                    </div>
                    <button
                      type="button"
                      title="Resolve"
                      data-testid={`resolve-${comment.id}`}
                      onClick={() => {
                        const id = sessionId();
                        if (!id) return;
                        void reviewActions
                          .resolveComment(id, comment.id)
                          .catch((e: Error) =>
                            notificationActions.addNotification('error', e.message),
                          );
                      }}
                      class="shrink-0 rounded p-1 text-muted-dark hover:text-ok hover:bg-hover-wash"
                    >
                      <Check class="w-3.5 h-3.5" />
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </div>
    </PanelShell>
  );
};

