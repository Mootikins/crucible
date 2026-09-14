/**
 * Changes — the review queue for the current session.
 *
 * Roots → files → hunks over the composed diff (`session_base` → worktree).
 * The queue IS the unreviewed subset: it drains as you review and, when it is
 * empty, you are simply browsing the session's changes. There is no mode
 * switch and no completion state, because there is nothing to switch between.
 *
 * Renders in the RIGHT edge region alongside Activity and Backlinks, which
 * puts it OUTSIDE the per-chat-tab `ChatProvider` — hence `useSessionSafe`
 * plus the global review store rather than `useChatSafe`, which would silently
 * hand back the inert fallback context.
 */
import { Component, For, Show, createMemo, createSignal } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import { HunkMergeView } from './HunkMergeView';
import { openFileInEditor } from '@/lib/file-actions';
import { notificationActions } from '@/stores/notificationStore';
import { reviewActions, reviewStore, toolCallLabel, useReviewSession } from '@/lib/review-store';
import {
  announceRefused,
  announceReject,
  announceRejectAll,
  confirmReject,
  confirmRejectAll,
  undoLastReject,
} from '@/lib/review-confirm';
import { hit } from '@/lib/touch';
import { hunkPath, hunkRangeLabel, isExternal, type ComposedHunk } from '@/lib/review-types';
import { Check, ChevronRight, MessageCircle, RefreshCw, Undo2 } from '@/lib/icons';

/** Files, in composed-diff order, with their hunks. */
interface FileGroup {
  root: string;
  path: string;
  absPath: string;
  hunks: ComposedHunk[];
}

function groupByFile(hunks: ComposedHunk[]): FileGroup[] {
  const groups: FileGroup[] = [];
  for (const h of hunks) {
    const absPath = hunkPath(h);
    const existing = groups.find((g) => g.absPath === absPath);
    if (existing) existing.hunks.push(h);
    else groups.push({ root: h.root, path: h.path, absPath, hunks: [h] });
  }
  return groups;
}

/** Roots in first-seen order, each with its files. */
function groupByRoot(hunks: ComposedHunk[]): { root: string; files: FileGroup[] }[] {
  const roots: { root: string; files: FileGroup[] }[] = [];
  for (const file of groupByFile(hunks)) {
    const existing = roots.find((r) => r.root === file.root);
    if (existing) existing.files.push(file);
    else roots.push({ root: file.root, files: [file] });
  }
  return roots;
}

/**
 * The hunks a bulk decision reaches: unreviewed and the agent's own. An
 * accepted or rejected hunk is already decided, and an external one is the
 * user's edit, which "Reject all" must never revert out from under them.
 */
function decidable(hunks: ComposedHunk[]): ComposedHunk[] {
  return hunks.filter((h) => h.state === 'unreviewed' && !isExternal(h));
}

const STATE_CLASS: Record<string, string> = {
  unreviewed: 'border-attention/50 bg-attention/10 text-attention',
  accepted: 'border-ok/50 bg-ok/10 text-ok',
  rejected: 'border-hairline bg-surface-elevated text-muted-dark',
};

const HunkRow: Component<{ sessionId: string; hunk: ComposedHunk }> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [commenting, setCommenting] = createSignal(false);
  const [body, setBody] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const external = () => isExternal(props.hunk);
  const range = () => hunkRangeLabel(props.hunk);

  // A refused mutation means the disk did not change — an unknown or stale
  // hunk, or an external one. Silence would read as a dropped click.
  const act = (fn: () => Promise<void>) => {
    if (busy()) return;
    setBusy(true);
    void fn()
      .catch((e: Error) => notificationActions.addNotification('error', e.message))
      .finally(() => setBusy(false));
  };

  const accept = () =>
    act(() => reviewActions.setState(props.sessionId, props.hunk.id, 'accepted'));
  // The confirm is the gate in front of the one destructive verb. The row's
  // button and the merge view's control share it, so neither is an unguarded
  // door to the same daemon call.
  const reject = () => {
    if (!confirmReject(props.hunk)) return;
    act(async () => {
      await reviewActions.reject(props.sessionId, props.hunk.id);
      announceReject(props.hunk, () => void undoLastReject(props.sessionId, [props.hunk]));
    });
  };

  const submitComment = () => {
    const text = body().trim();
    if (!text) return;
    act(async () => {
      await reviewActions.comment(props.sessionId, {
        root: props.hunk.root,
        path: props.hunk.path,
        line_start: props.hunk.current_range.start,
        line_end: props.hunk.current_range.end,
        body: text,
      });
      setBody('');
      setCommenting(false);
    });
  };

  return (
    <div class="border-t border-hairline" data-testid={`hunk-${props.hunk.id}`}>
      <div class="flex items-center gap-1.5 px-2 py-1">
        <button
          type="button"
          class="flex items-center gap-1 min-w-0 flex-1 text-left hover:bg-hover-wash rounded px-1 py-0.5"
          aria-expanded={open()}
          onClick={() => setOpen(!open())}
        >
          <ChevronRight
            class={`w-3 h-3 shrink-0 text-muted-dark transition-transform ${open() ? 'rotate-90' : ''}`}
          />
          <span class="text-floor font-mono text-muted shrink-0">{range()}</span>
          <span
            class={`text-floor px-1 py-px rounded border shrink-0 ${STATE_CLASS[props.hunk.state]}`}
          >
            {props.hunk.state}
          </span>
          {/* The whole point of the flag: without it a change the user already
              rejected, and the agent applied again, is indistinguishable from
              first-time work — and gets accepted out of fatigue. */}
          <Show when={props.hunk.reapplied}>
            <span
              class="text-floor px-1 py-px rounded border border-attention/50 bg-attention/10 text-attention shrink-0"
              title="You rejected this exact change before and the agent applied it again."
              data-testid={`hunk-reapplied-${props.hunk.id}`}
            >
              re-applied
            </span>
          </Show>
          <Show
            when={!external()}
            fallback={
              <span
                class="text-floor px-1 py-px rounded border border-hairline text-muted-dark shrink-0"
                title="Changed outside any tool call — your own editor, a formatter, or a plugin. Shown for context; not the agent's to undo."
                data-testid="hunk-external"
              >
                external
              </span>
            }
          >
            <span class="text-floor text-muted-dark truncate font-mono">
              {props.hunk.tool_call_ids.map(toolCallLabel).join(', ')}
            </span>
          </Show>
        </button>

        <Show when={props.hunk.state !== 'accepted'}>
          <button
            type="button"
            title="Accept"
            data-testid={`accept-${props.hunk.id}`}
            disabled={busy()}
            onClick={accept}
            class={`shrink-0 rounded p-1 text-muted-dark hover:text-ok hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
          >
            <Check class="w-3.5 h-3.5" />
          </button>
        </Show>
        {/* No reject for an external hunk: reverting one destroys the user's
            own concurrent edit while reporting that an agent edit was undone. */}
        <Show when={!external()}>
          <button
            type="button"
            title="Reject — reverts the change on disk and tells the agent"
            data-testid={`reject-${props.hunk.id}`}
            disabled={busy()}
            onClick={reject}
            class={`shrink-0 rounded p-1 text-muted-dark hover:text-error hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
          >
            <Undo2 class="w-3.5 h-3.5" />
          </button>
        </Show>
        <button
          type="button"
          title="Comment on these lines"
          data-testid={`comment-${props.hunk.id}`}
          onClick={() => setCommenting(!commenting())}
          class={`shrink-0 rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash ${hit()}`}
        >
          <MessageCircle class="w-3.5 h-3.5" />
        </button>
      </div>

      <Show when={commenting()}>
        <div class="px-3 pb-2 flex gap-1.5">
          <textarea
            rows="2"
            value={body()}
            data-testid={`comment-body-${props.hunk.id}`}
            onInput={(e) => setBody(e.currentTarget.value)}
            placeholder="Change this…"
            class="flex-1 rounded border border-hairline bg-surface-base px-2 py-1 text-floor text-shell-ink"
          />
          <button
            type="button"
            disabled={busy() || !body().trim()}
            onClick={submitComment}
            data-testid={`comment-submit-${props.hunk.id}`}
            class="self-end rounded border border-hairline px-2 py-1 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50"
          >
            Post
          </button>
        </div>
      </Show>

      <Show when={open()}>
        <div class="px-2 pb-2">
          {/* `before_content` is the hunk's session_base text and
              `after_content` its worktree text: the original and the document
              of one merge view. The view's controls decide through the daemon,
              the way the row's buttons do. */}
          <HunkMergeView
            hunk={props.hunk}
            onAccept={accept}
            onReject={external() ? undefined : reject}
          />
        </div>
      </Show>
    </div>
  );
};

export const ChangesPanel: Component = () => {
  const { currentSession } = useSessionSafe();
  const sessionId = () => currentSession()?.id;
  useReviewSession(sessionId);

  const [unreviewedOnly, setUnreviewedOnly] = createSignal(false);
  const [rebasing, setRebasing] = createSignal(false);
  const [bulkBusy, setBulkBusy] = createSignal(false);

  const state = () => reviewStore.session(sessionId());

  // One bulk decision in flight at a time. A second click while the daemon is
  // still applying the first would send the same ids again.
  const bulk = (fn: (id: string) => Promise<void>) => {
    const id = sessionId();
    if (!id || bulkBusy()) return;
    setBulkBusy(true);
    void fn(id)
      .catch((e: Error) => notificationActions.addNotification('error', e.message))
      .finally(() => setBulkBusy(false));
  };

  /** Accept every decidable hunk in `hunks`, in the order given. */
  const acceptAll = (hunks: ComposedHunk[]) => {
    const batch = decidable(hunks);
    if (batch.length === 0) return;
    bulk(async (id) => {
      const outcome = await reviewActions.setStates(
        id,
        batch.map((h) => h.id),
        'accepted',
      );
      announceRefused(outcome.failed, batch);
    });
  };

  /**
   * Reject every decidable hunk in `hunks` as ONE batch: one confirm, one
   * daemon call, one receipt with one undo. `label` names the scope in the
   * confirm.
   */
  const rejectAll = (hunks: ComposedHunk[], label: string) => {
    const batch = decidable(hunks);
    if (batch.length === 0) return;
    if (!confirmRejectAll(batch.length, label)) return;
    bulk(async (id) => {
      const outcome = await reviewActions.rejectMany(
        id,
        batch.map((h) => h.id),
      );
      // The names come from the batch, not from a re-list: a reverted hunk has
      // already left the composed diff by the time the daemon answers.
      if (outcome.applied.length > 0) {
        announceRejectAll(outcome.applied.length, () => void undoLastReject(id, batch));
      }
      announceRefused(outcome.failed, batch);
    });
  };
  // Named roots first, then the losses that name none — a journal that will not
  // read at all leaves nothing that can identify a repository, and that is
  // precisely the case a root-only list reports as "everything is fine".
  const degradedReasons = createMemo(() => [
    ...state().degraded.map((d) => `${d.root}: ${d.degraded ?? 'unreadable'}`),
    ...state()
      .skips.filter((s) => s.record.kind !== 'informational')
      .map((s) =>
        s.record.kind === 'root' ? `${s.record.root}: ${s.reason}` : `every root: ${s.reason}`,
      ),
  ]);
  // The filter and the count are the same predicate on purpose: "unreviewed
  // only" must show exactly what the badge says is owed. External hunks are in
  // neither — they are the user's own edits, not work the agent left behind —
  // but they stay in the unfiltered list, because the composed diff has to
  // remain honest about everything that changed.
  const visible = createMemo(() =>
    unreviewedOnly()
      ? state().hunks.filter((h) => h.state === 'unreviewed' && !isExternal(h))
      : state().hunks,
  );
  const roots = createMemo(() => groupByRoot(visible()));
  const unreviewed = () => reviewStore.unreviewedCount(sessionId());
  // The whole review, filter or no filter: the buttons decide what is owed,
  // not what is on screen.
  const decidableAll = createMemo(() => decidable(state().hunks));

  const openComments = createMemo(() => state().comments.filter((c) => !c.resolved));

  return (
    <PanelShell>
      <PanelHeader title="Changes" class="shrink-0">
        <div class="mt-1.5 flex items-center gap-2">
          <span class="text-floor text-muted-dark" data-testid="changes-count">
            {unreviewed()} unreviewed · {state().hunks.length} total
          </span>
          <label class="ml-auto flex items-center gap-1 text-floor text-muted-dark cursor-pointer">
            <input
              type="checkbox"
              checked={unreviewedOnly()}
              data-testid="changes-filter-unreviewed"
              onChange={(e) => setUnreviewedOnly(e.currentTarget.checked)}
            />
            unreviewed only
          </label>
          <button
            type="button"
            title="Refresh"
            data-testid="changes-refresh"
            disabled={!sessionId() || state().loading}
            onClick={() => {
              const id = sessionId();
              if (id) void reviewActions.refresh(id);
            }}
            class="rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50"
          >
            <RefreshCw class={`w-3.5 h-3.5 ${state().loading ? 'animate-spin' : ''}`} />
          </button>
        </div>
        {/* The review-wide pair. Disabled, not hidden, so the panel keeps its
            shape while the queue drains. */}
        <div class="mt-1 flex items-center gap-1.5">
          <button
            type="button"
            title="Accept every unreviewed change in this review"
            data-testid="changes-accept-all"
            disabled={bulkBusy() || decidableAll().length === 0}
            onClick={() => acceptAll(state().hunks)}
            class={`rounded border border-hairline px-2 py-0.5 text-floor text-muted-dark hover:text-ok hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
          >
            Accept all
          </button>
          <button
            type="button"
            title="Reject every unreviewed change in this review — reverts them on disk and tells the agent"
            data-testid="changes-reject-all"
            disabled={bulkBusy() || decidableAll().length === 0}
            onClick={() => rejectAll(state().hunks, 'every file in this review')}
            class={`rounded border border-hairline px-2 py-0.5 text-floor text-muted-dark hover:text-error hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
          >
            Reject all
          </button>
        </div>
      </PanelHeader>

      <div class="flex-1 overflow-y-auto">
        <Show
          when={sessionId()}
          fallback={<p class="p-3 text-xs text-muted-dark">No session selected.</p>}
        >
          <Show when={state().error}>
            <p class="px-3 py-2 text-xs text-error" data-testid="changes-error">
              {state().error}
            </p>
          </Show>

          {/* Before the empty state, and instead of it. A degraded root
              contributes ZERO hunks while the gate holds every write under it,
              so "No changes in this session yet" is the exact wrong sentence
              for the exact moment nothing can proceed — and reviewing cannot
              clear it, because there is nothing in the queue to review. */}
          <Show when={reviewStore.isDegraded(sessionId())}>
            <div
              class="m-2 rounded border border-attention/50 bg-attention/10 px-3 py-2"
              data-testid="changes-degraded"
            >
              <p class="text-xs text-attention">
                This session's change history cannot be read, so writes are held. Reviewing will not
                release them.
              </p>
              <ul class="mt-1 space-y-0.5">
                <For each={degradedReasons()}>
                  {(reason) => (
                    <li class="text-floor text-muted-dark font-mono break-words">{reason}</li>
                  )}
                </For>
              </ul>
              <button
                type="button"
                data-testid="changes-rebase"
                disabled={rebasing()}
                onClick={() => {
                  const id = sessionId();
                  if (!id) return;
                  setRebasing(true);
                  void reviewActions
                    .rebase(id)
                    .catch((e: Error) => notificationActions.addNotification('error', e.message))
                    .finally(() => setRebasing(false));
                }}
                class="mt-2 rounded border border-hairline px-2 py-1 text-floor text-shell-ink hover:bg-hover-wash disabled:opacity-50"
              >
                Accept the worktree as the new base
              </button>
            </div>
          </Show>

          <Show
            when={
              state().loaded && state().hunks.length === 0 && !reviewStore.isDegraded(sessionId())
            }
          >
            <p class="p-3 text-xs text-muted-dark" data-testid="changes-empty">
              No changes in this session yet.
            </p>
          </Show>

          {/* The drained queue. Not a "done" state — the changes are still
              here to browse, there is simply nothing owed. */}
          <Show when={state().hunks.length > 0 && visible().length === 0}>
            <p class="p-3 text-xs text-muted-dark" data-testid="changes-all-reviewed">
              Nothing left to review.
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
                    <div class="border-b border-hairline">
                      <div class="flex items-center gap-1 pr-2">
                        <button
                          type="button"
                          data-testid={`changes-file-${file.path}`}
                          onClick={() => {
                            openFileInEditor(file.absPath, file.path.split('/').pop());
                            reviewActions.reveal(file.absPath, file.hunks[0].current_range.start);
                          }}
                          class="flex-1 min-w-0 flex items-center gap-2 px-3 py-1.5 text-left hover:bg-hover-wash"
                        >
                          <span class="flex-1 min-w-0 truncate text-xs font-mono text-shell-ink">
                            {file.path}
                          </span>
                          <span class="shrink-0 text-floor text-muted-dark">
                            {file.hunks.length}
                          </span>
                        </button>
                        {/* Per-file pair. Hidden once the file has nothing left
                            to decide: a decided file is history, not a queue. */}
                        <Show when={decidable(file.hunks).length > 0}>
                          <button
                            type="button"
                            title={`Accept every unreviewed change in ${file.path}`}
                            data-testid={`accept-all-${file.path}`}
                            disabled={bulkBusy()}
                            onClick={() => acceptAll(file.hunks)}
                            class={`shrink-0 rounded px-1.5 py-0.5 text-floor text-muted-dark hover:text-ok hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
                          >
                            Accept all
                          </button>
                          <button
                            type="button"
                            title={`Reject every unreviewed change in ${file.path} — reverts them on disk and tells the agent`}
                            data-testid={`reject-all-${file.path}`}
                            disabled={bulkBusy()}
                            onClick={() => rejectAll(file.hunks, file.path)}
                            class={`shrink-0 rounded px-1.5 py-0.5 text-floor text-muted-dark hover:text-error hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
                          >
                            Reject all
                          </button>
                        </Show>
                      </div>
                      <For each={file.hunks}>
                        {(hunk) => <HunkRow sessionId={sessionId()!} hunk={hunk} />}
                      </For>
                    </div>
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

