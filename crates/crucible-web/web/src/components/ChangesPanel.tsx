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
import { Component, For, Show, createMemo, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import { HunkMergeView } from './HunkMergeView';
import { openFileInEditor } from '@/lib/file-actions';
import { notificationActions } from '@/stores/notificationStore';
import { reviewActions, reviewStore, toolCallLabel, useReviewSession } from '@/lib/review-store';
import { hit } from '@/lib/touch';
import { conflictActions, conflictStore, openConflict } from '@/lib/conflicts';
import {
  hunkPath,
  hunkRangeLabel,
  isExternal,
  type ComposedHunk,
  type ReviewScope,
} from '@/lib/review-types';
import { AlertTriangle, Check, ChevronRight, MessageCircle, RefreshCw } from '@/lib/icons';
import { useProposals } from '@/lib/query/proposals';
import { authorLabel } from '@/lib/proposal-api';
import { openDiff } from '@/lib/panel-actions';

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

  // A refused comment means nothing was stored. Silence would read as a
  // dropped click.
  const act = (fn: () => Promise<void>) => {
    if (busy()) return;
    setBusy(true);
    void fn()
      .catch((e: Error) => notificationActions.addNotification('error', e.message))
      .finally(() => setBusy(false));
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
              of one merge view. */}
          <HunkMergeView hunk={props.hunk} />
        </div>
      </Show>
    </div>
  );
};

/**
 * The two scopes, in the order the control draws them. A filter over one
 * composed diff, decided by the daemon: a hunk the current turn extended and
 * an older turn began shows whole under "Turn".
 */
const SCOPES: { scope: ReviewScope; label: string; title: string }[] = [
  { scope: 'session', label: 'Session', title: 'Every change this session made' },
  { scope: 'turn', label: 'Turn', title: 'Only the changes the current turn made' },
];

export const ChangesPanel: Component = () => {
  const { currentSession } = useSessionSafe();
  const sessionId = () => currentSession()?.session_id;
  useReviewSession(sessionId);

  const [unreviewedOnly, setUnreviewedOnly] = createSignal(false);

  const state = () => reviewStore.session(sessionId());
  const scope = () => reviewStore.scope(sessionId());

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
        {/* The scope control: the daemon decides what the turn holds, the
            panel only asks. */}
        <div class="mt-1 flex items-center gap-1.5">
          <div
            role="group"
            aria-label="Scope"
            class="flex items-center rounded border border-hairline text-floor"
          >
            <For each={SCOPES}>
              {(option) => (
                <button
                  type="button"
                  title={option.title}
                  data-testid={`changes-scope-${option.scope}`}
                  aria-pressed={scope() === option.scope}
                  onClick={() => {
                    const id = sessionId();
                    if (id) void reviewActions.setScope(id, option.scope);
                  }}
                  class={`px-2 py-0.5 hover:bg-hover-wash ${
                    scope() === option.scope
                      ? 'bg-hover-wash text-shell-ink'
                      : 'text-muted-dark hover:text-shell-ink'
                  } ${hit()}`}
                >
                  {option.label}
                </button>
              )}
            </For>
          </div>
        </div>
      </PanelHeader>

      <div class="flex-1 overflow-y-auto">
        {/* Above the roots, and OUTSIDE the session gate. A hunk drains as
            you decide it; a conflict does not, it belongs to no session's
            composed diff, and on a desktop there is no offline badge to carry
            it — so a conflict that only showed under a selected session would
            be a write nothing lists. The counts below stay about the session. */}
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

        {/* Beside the conflicts, and outside the session gate for the same
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

          {/* Before the empty state, and instead of it. A degraded root
              contributes ZERO hunks, so "No changes in this session yet" is
              the wrong sentence for a record that cannot be read. */}
          <Show when={reviewStore.isDegraded(sessionId())}>
            <div
              class="m-2 rounded border border-attention/50 bg-attention/10 px-3 py-2"
              data-testid="changes-degraded"
            >
              <p class="text-xs text-attention">
                This session's change history cannot be read, so this list is incomplete.
              </p>
              <ul class="mt-1 space-y-0.5">
                <For each={degradedReasons()}>
                  {(reason) => (
                    <li class="text-floor text-muted-dark font-mono break-words">{reason}</li>
                  )}
                </For>
              </ul>
            </div>
          </Show>

          <Show
            when={
              state().loaded && state().hunks.length === 0 && !reviewStore.isDegraded(sessionId())
            }
          >
            <p class="p-3 text-xs text-muted-dark" data-testid="changes-empty">
              No changes in this {scope()} yet.
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

