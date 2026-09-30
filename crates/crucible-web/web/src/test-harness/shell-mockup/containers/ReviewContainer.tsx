/** A diff pane: decisions belong to a proposal or a file, never a hunk. */
import { For, Show, createSignal, createEffect, onCleanup, type Component } from 'solid-js';
import { openNote } from '../actions';
import {
  addReviewComment,
  decideProposal,
  proposalPending,
  review,
  reviewFiles,
  setReview,
  type ReviewSource,
} from '../review';
import { state } from '../state';
import { MessageSquare } from '@/lib/icons';
import { Button } from '../components/primitives/Button';
import { DecisionButtons } from '../components/primitives/DecisionButtons';
import { DiffRows } from '../components/primitives/DiffRows';
import { Caret } from '../components/primitives/Caret';
import { DiffStat } from '../components/session/DiffStat';

export const ReviewContainer: Component<{ source: ReviewSource; sid: string; path?: string }> = (
  props,
) => {
  const proposal = () => props.source === 'proposal';
  const files = () => reviewFiles(proposal() ? 's3' : props.sid);
  const [commentPath, setCommentPath] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal('');
  const [hiddenHunks, setHiddenHunks] = createSignal<ReadonlySet<string>>(new Set());
  const [folded, setFolded] = createSignal<ReadonlySet<string>>(new Set());
  const toggleFile = (path: string) =>
    setFolded((current) => {
      const next = new Set(current);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  const decideAll = (accept: boolean) =>
    proposalPending().forEach((path) => decideProposal(path, accept));
  let root!: HTMLDivElement;
  createEffect(() => {
    const path = props.path;
    if (!path) return;
    const frame = requestAnimationFrame(() =>
      Array.from(root.querySelectorAll<HTMLElement>('[data-review-file]'))
        .find((el) => el.dataset.reviewFile === path)
        ?.scrollIntoView({ block: 'start' }),
    );
    onCleanup(() => cancelAnimationFrame(frame));
  });
  return (
    <div class="mk-scroll" ref={root}>
      <section
        class="mk-changes mk-review"
        data-testid={proposal() ? 'mock-review' : 'mock-changes'}
      >
        <header class="mk-review-title">
          <h2>{proposal() ? 'Tighten the knowledge notes' : 'Changes'}</h2>
        </header>
        <div class="mk-review-toolbar">
          <div class="mk-review-summary">
            <span>
              {files().length} {files().length === 1 ? 'file' : 'files'}
            </span>
            <DiffStat
              add={files().reduce((n, f) => n + f.add, 0)}
              del={files().reduce((n, f) => n + f.del, 0)}
            />
            <span class="mk-review-meta">
              {proposal()
                ? 'Reflection pass'
                : (state.sessions[props.sid]?.title ?? 'Server cleanup')}
            </span>
          </div>
          <Show when={proposal()}>
            <div class="mk-review-actions">
              <DecisionButtons
                primary
                acceptLabel="Accept all"
                rejectLabel="Reject all"
                disabled={!proposalPending().length}
                onAccept={() => decideAll(true)}
                onReject={() => decideAll(false)}
              />
            </div>
          </Show>
        </div>
        <For each={files()}>
          {(file) => (
            <article class="mk-review-file" data-review-file={file.path}>
              <div class="mk-review-file-head">
                <div class="mk-review-file-name">
                  <button
                    type="button"
                    class="mk-iconbtn mk-diff-toggle"
                    aria-expanded={!folded().has(file.path)}
                    aria-label={`${folded().has(file.path) ? 'Expand' : 'Collapse'} changes for ${file.path}`}
                    onClick={() => toggleFile(file.path)}
                  >
                    <Caret open={!folded().has(file.path)} />
                  </button>
                  <button class="mk-f" type="button" onClick={() => openNote(file.path)}>
                    {file.path}
                    {file.code ? '' : '.md'}
                  </button>
                  <DiffStat add={file.add} del={file.del} />
                </div>
                <div class="mk-review-file-actions">
                  <button
                    type="button"
                    aria-label="Comment"
                    class="mk-btn sm ghost mk-comment-toggle"
                    aria-pressed={commentPath() === file.path}
                    onClick={() => {
                      setCommentPath(commentPath() === file.path ? null : file.path);
                      setDraft('');
                    }}
                  >
                    <MessageSquare class="mk-i" />
                    Comment
                  </button>
                  <Show when={proposal()}>
                    <Show
                      when={!review.files[file.path]}
                      fallback={<span class="mk-quiet">{review.files[file.path]}</span>}
                    >
                      <DecisionButtons
                        acceptLabel="Accept file"
                        rejectLabel="Reject file"
                        onAccept={() => decideProposal(file.path, true)}
                        onReject={() => decideProposal(file.path, false)}
                      />
                    </Show>
                  </Show>
                </div>
              </div>
              <Show when={!folded().has(file.path)}>
                <For each={file.hunks}>
                  {(h) => (
                    <div class="mk-review-diff" classList={{ code: file.code }}>
                      <Show when={file.code}>
                        <button
                          class="mk-review-hunk-toggle"
                          type="button"
                          aria-expanded={!hiddenHunks().has(h.id)}
                          onClick={() =>
                            setHiddenHunks((current) => {
                              const next = new Set(current);
                              if (!next.delete(h.id)) next.add(h.id);
                              return next;
                            })
                          }
                        >
                          <Caret open={!hiddenHunks().has(h.id)} />
                          Lines {h.rows[0]?.newLineNum ?? h.rows[0]?.oldLineNum}–
                          {h.rows.at(-1)?.newLineNum ?? h.rows.at(-1)?.oldLineNum}
                        </button>
                      </Show>
                      <Show when={!hiddenHunks().has(h.id)}>
                        <DiffRows
                          rows={h.rows}
                          emphasis
                          fileName={file.code ? file.path : undefined}
                        />
                      </Show>
                    </div>
                  )}
                </For>
              </Show>
              <Show when={commentPath() === file.path}>
                <form
                  class="mk-review-comment"
                  onSubmit={(e) => {
                    e.preventDefault();
                    addReviewComment(file.path, draft());
                    setCommentPath(null);
                  }}
                >
                  <label>
                    <textarea
                      aria-label="Review comment"
                      placeholder="Add a comment…"
                      value={draft()}
                      onInput={(e) => setDraft(e.currentTarget.value)}
                    />
                  </label>
                  <div class="mk-review-actions">
                    <Button variant="ghost" onClick={() => setCommentPath(null)}>
                      Cancel
                    </Button>
                    <button type="submit" class="mk-btn sm primary" disabled={!draft().trim()}>
                      <MessageSquare class="mk-i" />
                      Comment
                    </button>
                  </div>
                </form>
              </Show>
              <For each={review.comments.filter((c) => c.path === file.path && !c.resolved)}>
                {(c) => (
                  <div class="mk-review-comment">
                    <p>{c.text}</p>
                    <div class="mk-review-actions">
                      <small class="mk-quiet">
                        {c.attached ? 'Attached to chat' : 'Comment saved'}
                      </small>
                      <Show when={!c.attached}>
                        <Button
                          variant="ghost"
                          onClick={() =>
                            setReview('comments', (x) => x.id === c.id, 'attached', true)
                          }
                        >
                          Attach
                        </Button>
                      </Show>
                      <Button
                        variant="ghost"
                        onClick={() =>
                          setReview('comments', (x) => x.id === c.id, 'resolved', true)
                        }
                      >
                        Resolve
                      </Button>
                    </div>
                  </div>
                )}
              </For>
            </article>
          )}
        </For>
      </section>
    </div>
  );
};
