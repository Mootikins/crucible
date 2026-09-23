/**
 * One diffset in the centre region: a header, then one section for each file.
 *
 * The daemon computes the diffset and the counts. The panel asks for the two
 * texts of a file only when its section is expanded and near the viewport, and
 * CodeMirror computes the hunks. A branch with 400 files therefore does not
 * send 800 texts at once.
 *
 * A diff is a code view. The body is monospace, it never reflows, and the
 * prose features of the note editor stay off. The change bar is the 3 px
 * gutter of `@codemirror/merge`. The panel draws no chip.
 *
 * A focus target in the tab metadata names one file. The panel expands that
 * file and scrolls to it, so a click in the Changes panel or on a tool card
 * shows the clicked file.
 *
 * Each hunk has a header row with its patch range. A click on the header
 * hides the hunk or shows it again. The panel keeps that choice, so that a new
 * editor for the file (a wrap or a layout change) keeps it too.
 *
 * A line comment starts on the line numbers (`diff-comments.tsx`). The daemon
 * stores it, and lists it with an outdated flag. **Resolve** on a comment
 * marks it resolved, and the open list then leaves it out. An outdated comment shows at
 * the end of its file, because its text is no longer in the file.
 *
 * **Comment** also attaches the comment to the composer of the chat of this
 * pane: a session record belongs to its own session, and any other diffset
 * takes the session that was active when the tab opened. The chip in the
 * composer carries a reference, and the daemon builds the context of the
 * comment when the message goes. A pane with no chat still stores the
 * comment, and says that no chat receives it.
 *
 * The chip and the stored comment are one thing. **Attach** on a comment with
 * no chip puts it back in the composer, and the `×` of a chip deletes the
 * comment, so the pane and the composer always agree.
 *
 * A proposal adds its decisions: Accept all and Reject all in the header, and
 * Accept and Reject on each file. A conflicted proposal shows a `ConflictView`
 * for each conflicted file instead of the files, and **Accept resolution**
 * gives the settled text to the daemon.
 */
import {
  Component,
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  untrack,
  type JSX,
} from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { MergeView } from '@codemirror/merge';
import { PanelShell } from './PanelShell';
import {
  hidesFinalNewline,
  mergeViewExtensions,
  setHiddenHunks,
  type MergeCollapse,
} from '@/lib/merge-view';
import {
  commentChipLabel,
  diffsetLabel,
  focusMatches,
  quickfixList,
  type CommentSide,
  type DiffComment,
  type DiffFileEntry,
  type DiffFocusRequest,
  type DiffFileText,
  type DiffsetSource,
  type ListedComment,
} from '@/lib/diffset';
import {
  invalidateDiffset,
  useDiffComments,
  useDiffFile,
  useDiffset,
  usePostDiffComment,
  useResolveDiffComment,
} from '@/lib/query/diff';
import { openDiff } from '@/lib/panel-actions';
import { useSession } from '@/lib/query/sessions';
import { composerComments } from '@/stores/composerComments';
import { isPending, stateLabel, type FileConflict, type Proposal } from '@/lib/proposal-api';
import { useProposal, useProposalDecision, type ProposalDecision } from '@/lib/query/proposals';
import { notificationActions } from '@/stores/notificationStore';
import { ConflictView } from './ConflictView';
import { UnreadableRoots } from './UnreadableRoots';
import {
  commentExtensions,
  setComments,
  spanLabel,
  type CommentHost,
  type LineSpan,
} from './diff-comments';
import {
  ArrowRight,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Columns2,
  Copy,
  MessageSquareText,
  RefreshCw,
  Rows2,
  WrapText,
} from '@/lib/icons';
import { fileIconFor } from '@/lib/file-icons';
import { hit } from '@/lib/touch';

/** A file with more changed lines than this starts collapsed. */
const LARGE_FILE_LINES = 400;

/** The unchanged-line collapse of the design: 3 lines of context, 4 at least. */
const COLLAPSE: MergeCollapse = { margin: 3, minSize: 4 };

/**
 * The added and removed line counts, in the diff colors. A count of zero is
 * left out, as a patch summary does.
 */
function ChangeCounts(props: { added: number; removed: number; testId: string; class?: string }) {
  return (
    <span class={`text-floor font-mono ${props.class ?? ''}`} data-testid={props.testId}>
      <Show when={props.added > 0}>
        <span class="text-ok">+{props.added}</span>
      </Show>
      <Show when={props.added > 0 && props.removed > 0}> </Show>
      <Show when={props.removed > 0}>
        <span class="text-error">−{props.removed}</span>
      </Show>
    </span>
  );
}

export interface DiffPanelProps {
  /** The tab metadata that `openDiff` writes. */
  source?: DiffsetSource;
  /** The file to scroll to and expand, from the tab metadata. */
  focus?: DiffFocusRequest;
  /** The chat that a new comment attaches to, from the tab metadata. */
  session?: string;
}

/**
 * The key of one file in the panel. A session record can span more than one
 * root, and two roots can hold the same relative path.
 */
function fileKey(file: Pick<DiffFileEntry, 'root' | 'path'>): string {
  return `${file.root}:${file.path}`;
}

/** An icon button of the toolbar or of a file row. */
const iconButton =
  'flex shrink-0 items-center justify-center gap-1 rounded p-1 text-muted-dark hover:bg-hover-wash hover:text-shell-ink disabled:opacity-50 disabled:hover:bg-transparent disabled:hover:text-muted-dark focus-ring';
/** The state of a toggle in the toolbar: on, or off. */
const pressed = (on: boolean) => (on ? 'bg-control text-shell-ink hover:bg-control' : '');

/** A decision button. The accept form has a fill; the reject form has none. */
const decisionButton = (accept: boolean) =>
  `flex h-6 shrink-0 items-center rounded px-2 text-floor disabled:opacity-50 focus-ring ${
    accept
      ? 'bg-control text-shell-ink hover:text-shell-ink hover:bg-hover-wash'
      : 'text-muted hover:bg-hover-wash hover:text-shell-ink'
  }`;

/** A thin rule between two groups of the toolbar. */
const Rule = () => <span aria-hidden="true" class="mx-1 h-4 w-px shrink-0 bg-hairline-strong" />;

/** A git ref in mono, or the prose name of the default when it has none. */
const Ref: Component<{ name: string | null | undefined; fallback: string }> = (props) => (
  <Show when={props.name} fallback={<span class="text-shell-ink">{props.fallback}</span>}>
    {(name) => <span class="min-w-0 truncate font-mono text-shell-ink">{name()}</span>}
  </Show>
);

/** The source when it is of one kind, or null. */
function ofKind<K extends DiffsetSource['kind']>(
  source: DiffsetSource,
  kind: K,
): Extract<DiffsetSource, { kind: K }> | null {
  return source.kind === kind ? (source as Extract<DiffsetSource, { kind: K }>) : null;
}

/** The first block of an id. The full id is in the tooltip. */
const shortId = (id: string) => id.split('-')[0];

/**
 * What the diffset compares, in the UI font. Only a ref or an id is in mono.
 * A proposal shows its title and its state: its id is in the tooltip.
 */
const SourceLabel: Component<{ source: DiffsetSource; proposal?: Proposal }> = (props) => (
  <span
    class="flex min-w-0 items-center gap-1.5 whitespace-nowrap text-xs"
    data-testid="diff-source"
    title={diffsetLabel(props.source)}
  >
    <Switch>
      <Match when={ofKind(props.source, 'branch')}>
        {(branch) => (
          <>
            <Ref name={branch().head} fallback="Working tree" />
            <ArrowRight class="h-3 w-3 shrink-0 text-muted-dark" aria-hidden="true" />
            <span class="sr-only">against</span>
            <Ref name={branch().base} fallback="Default branch" />
          </>
        )}
      </Match>
      <Match when={ofKind(props.source, 'session_record')}>
        {(record) => (
          <>
            <span class="text-shell-ink">Session</span>
            <span class="min-w-0 truncate font-mono text-muted">{record().session}</span>
          </>
        )}
      </Match>
      <Match when={ofKind(props.source, 'proposal')}>
        {(proposal) => (
          <span class="flex min-w-0 items-center gap-1.5" data-testid="proposal-bar">
            <Show
              when={props.proposal}
              fallback={
                <>
                  <span class="text-shell-ink">Proposal</span>
                  <span class="font-mono text-muted">{shortId(proposal().id)}</span>
                </>
              }
            >
              {(value) => (
                <>
                  <span class="min-w-0 truncate text-shell-ink" data-testid="proposal-title">
                    {value().title}
                  </span>
                  <span class="shrink-0 text-floor text-muted" data-testid="proposal-state">
                    {stateLabel(value().state)}
                  </span>
                </>
              )}
            </Show>
          </span>
        )}
      </Match>
    </Switch>
  </span>
);

/**
 * The chat that a new comment of this pane attaches to.
 *
 * The name is the title of the session, else its short id. A pane with no
 * chat says so, because a comment made there reaches no agent.
 */
const ChatTarget: Component<{ session?: string }> = (props) => {
  const session = useSession(() => props.session ?? null);
  return (
    <span
      class="flex min-w-0 shrink items-center gap-1 whitespace-nowrap text-floor text-muted-dark"
      data-testid="diff-chat-target"
      title={
        props.session
          ? 'A comment of this pane attaches to this chat'
          : 'This pane has no chat. A comment is stored, and no chat receives it.'
      }
    >
      <MessageSquareText class="h-3 w-3 shrink-0" aria-hidden="true" />
      <span class="min-w-0 truncate">{chatTargetName(props.session, session.data?.title)}</span>
    </span>
  );
};

/** "No chat", or the title of the session, or its short id. */
function chatTargetName(session: string | undefined, title: string | null | undefined): string {
  if (!session) return 'No chat';
  return title?.trim() ? title : shortId(session);
}

export const DiffPanel: Component<DiffPanelProps> = (props) => {
  return (
    <PanelShell>
      <Show
        when={props.source}
        fallback={<p class="p-3 text-xs text-muted-dark">This tab names no diff.</p>}
      >
        {(source) => (
          <Show
            when={proposalSource(source())}
            fallback={<DiffsetView source={source()} focus={props.focus} session={props.session} />}
          >
            {(proposal) => (
              <ProposalDiffsetView
                source={proposal()}
                focus={props.focus}
                session={props.session}
              />
            )}
          </Show>
        )}
      </Show>
    </PanelShell>
  );
};

type ProposalSource = Extract<DiffsetSource, { kind: 'proposal' }>;

function proposalSource(source: DiffsetSource): ProposalSource | null {
  return source.kind === 'proposal' ? source : null;
}

/** What the diffset view needs to show the decisions of a proposal. */
interface ProposalControls {
  proposal: () => Proposal | undefined;
  /** Sends the decision. It tells the person of a failure, and never rejects. */
  decide: (decision: ProposalDecision) => Promise<void>;
  busy: () => boolean;
}

/**
 * The key of the text that a conflict was merged against: a 32-bit FNV-1a
 * hash. A new disk text is a new conflict, so the view builds it again.
 */
function textKey(text: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return `${text.length}:${(hash >>> 0).toString(16)}`;
}

const ProposalDiffsetView: Component<{
  source: ProposalSource;
  focus?: DiffFocusRequest;
  session?: string;
}> = (props) => {
  const proposal = useProposal(() => props.source.id);
  const decision = useProposalDecision(() => props.source.id);
  const decide = (value: ProposalDecision): Promise<void> =>
    decision
      .mutateAsync(value)
      .then((reply) => {
        // A decision on some files moves them into a new proposal. A conflict
        // there needs its own pane.
        if (reply.id !== props.source.id && reply.state.kind === 'conflicted') {
          openDiff({ kind: 'proposal', id: reply.id }, undefined, props.session ?? undefined);
        }
      })
      .catch((e: Error) => {
        notificationActions.addNotification('error', e.message);
      });
  const controls: ProposalControls = {
    proposal: () => proposal.data,
    decide,
    busy: () => decision.isPending,
  };
  return (
    <DiffsetView
      source={props.source}
      proposal={controls}
      focus={props.focus}
      session={props.session}
    />
  );
};

/** Whether the files of the proposal take a decision now. */
function decidable(proposal: Proposal | undefined): boolean {
  return !!proposal && isPending(proposal.state) && proposal.state.kind !== 'conflicted';
}

/** Accept all and Reject all, at the end of the toolbar. */
const ProposalActions: Component<{ controls: ProposalControls }> = (props) => {
  const proposal = () => props.controls.proposal();
  // A superseded or conflicted proposal can still be rejected.
  const rejectable = () => {
    const state = proposal()?.state;
    return !!state && (isPending(state) || state.kind === 'superseded');
  };
  return (
    <Show when={decidable(proposal()) || rejectable()}>
      <Rule />
      <div class="flex shrink-0 items-center gap-1">
        <Show when={rejectable()}>
          <button
            type="button"
            title="Reject every file of the proposal. No file changes."
            data-testid="proposal-reject-all"
            disabled={props.controls.busy()}
            onClick={() => void props.controls.decide({ kind: 'reject' })}
            class={`${decisionButton(false)} ${hit()}`}
          >
            Reject all
          </button>
        </Show>
        <Show when={decidable(proposal())}>
          <button
            type="button"
            title="Write every file of the proposal"
            data-testid="proposal-accept-all"
            disabled={props.controls.busy()}
            onClick={() => void props.controls.decide({ kind: 'accept' })}
            class={`${decisionButton(true)} ${hit()}`}
          >
            Accept all
          </button>
        </Show>
      </div>
    </Show>
  );
};

/** One `ConflictView` for each conflicted file of a proposal. */
const ProposalConflicts: Component<{ files: FileConflict[]; controls: ProposalControls }> = (
  props,
) => (
  <For each={props.files}>
    {(file) => (
      <section
        class="h-96 border-b border-hairline"
        data-testid={`proposal-conflict-${file.root}:${file.path}`}
      >
        <ConflictView
          path={file.path}
          mergedContent={file.merged_text}
          regions={file.regions}
          baseHash={textKey(file.disk_text)}
          saveLabel="Accept resolution"
          hint="The proposed text is in the note. Choose what to keep where the note on disk says something else."
          onSave={(text) => props.controls.decide({ kind: 'resolve', path: file.path, text })}
        />
      </section>
    )}
  </For>
);

/**
 * The hidden hunks of one file: the default of the toolbar, and the choice of
 * the user for each hunk, by its label.
 */
interface HunkChoice {
  hidden: boolean;
  own: Readonly<Record<string, boolean>>;
}

/**
 * What the pane does with the chip of one stored comment.
 *
 * The chip and the comment are one thing. A comment with no chip can attach
 * itself again; the `×` of a chip deletes the comment.
 */
interface ChipActions {
  attached: (commentId: string) => boolean;
  attach: (comment: DiffComment) => void;
}

interface DiffsetViewProps {
  source: DiffsetSource;
  proposal?: ProposalControls;
  focus?: DiffFocusRequest;
  /** The chat that a new comment attaches to. */
  session?: string;
}

const DiffsetView: Component<DiffsetViewProps> = (props) => {
  const diffset = useDiffset(() => props.source);
  const comments = useDiffComments(() => props.source);
  const [split, setSplit] = createSignal(false);
  const [wrap, setWrap] = createSignal(true);
  // The choice of the user for each file. A path without a choice takes the
  // size rule, so a refresh that adds a file applies the rule to it.
  const [expanded, setExpanded] = createSignal<Record<string, boolean>>({});

  const files = () => diffset.data?.files ?? [];
  const totals = createMemo(() =>
    files().reduce((sum, f) => ({ added: sum.added + f.added, removed: sum.removed + f.removed }), {
      added: 0,
      removed: 0,
    }),
  );
  const isExpanded = (file: DiffFileEntry) =>
    expanded()[fileKey(file)] ?? file.added + file.removed <= LARGE_FILE_LINES;
  const toggle = (file: DiffFileEntry) =>
    setExpanded((prev) => ({ ...prev, [fileKey(file)]: !isExpanded(file) }));
  // The hunks. Collapse all and Expand all set the default of every hunk and
  // clear the choices for single hunks.
  const [hunksHidden, setHunksHidden] = createSignal(false);
  const [hunkChoices, setHunkChoices] = createSignal<Record<string, Record<string, boolean>>>({});
  const hunksOf = (file: DiffFileEntry): HunkChoice => ({
    hidden: hunksHidden(),
    own: hunkChoices()[fileKey(file)] ?? {},
  });
  const toggleHunk = (file: DiffFileEntry, label: string) =>
    setHunkChoices((prev) => {
      const key = fileKey(file);
      const own = prev[key] ?? {};
      return { ...prev, [key]: { ...own, [label]: !(own[label] ?? hunksHidden()) } };
    });
  const toggleAllHunks = () => {
    setHunksHidden(!hunksHidden());
    setHunkChoices({});
  };

  // The focus target: expand its file, then scroll to its section. Each
  // request applies once, when the diffset lists the file, so a refresh does
  // not scroll again. A target that the diffset does not list does nothing.
  let body!: HTMLDivElement;
  let focusedSeq: number | undefined;
  createEffect(() => {
    const focus = props.focus;
    if (!focus || focus.seq === focusedSeq) return;
    const file = files().find((f) => focusMatches(f, focus));
    if (!file) return;
    focusedSeq = focus.seq;
    const key = fileKey(file);
    setExpanded((prev) => ({ ...prev, [key]: true }));
    // The section exists already. Scroll after the expansion renders, so the
    // section is at the top with its body below it.
    queueMicrotask(() => {
      const section = [...body.querySelectorAll<HTMLElement>('section[data-file-key]')].find(
        (el) => el.dataset.fileKey === key,
      );
      section?.scrollIntoView?.({ block: 'start' });
    });
  });

  const openComments = () => (comments.data ?? []).map((l) => l.comment).filter((c) => !c.resolved);
  const commentsOf = (file: DiffFileEntry) =>
    (comments.data ?? []).filter(
      (l) => !l.comment.resolved && fileKey(l.comment) === fileKey(file),
    );
  const resolution = useResolveDiffComment();
  // The widget shows the reason of a refusal, so the promise rejects with it.
  const resolve = (commentId: string): Promise<void> =>
    resolution.mutateAsync({ source: props.source, commentId }).then(() => undefined);
  // The chip of a comment in the composer of the chat of this pane. Attach
  // puts it back; the `×` of the chip deletes the comment (`ChatInput`).
  const chips: ChipActions = {
    attached: (commentId) => composerComments.has(props.session, commentId),
    attach: (comment) => {
      const session = props.session;
      if (!session) return;
      composerComments.attach(session, {
        id: comment.id,
        source: props.source,
        label: commentChipLabel(comment),
        title: `${comment.path} · ${comment.body}`,
      });
    },
  };
  const copyComments = () =>
    void navigator.clipboard?.writeText(quickfixList(openComments())).catch(() => undefined);

  // The conflicted files of a proposal, or null for any other state.
  const conflicts = () => {
    const state = props.proposal?.proposal()?.state;
    return state?.kind === 'conflicted' ? state.files : null;
  };
  // The decision on one file, while the proposal takes one.
  const fileDecision = () => {
    const controls = props.proposal;
    if (!controls || !decidable(controls.proposal())) return undefined;
    return {
      busy: controls.busy(),
      run: (kind: 'accept' | 'reject', file: DiffFileEntry) =>
        void controls.decide({ kind, paths: [file.path] }),
    };
  };

  return (
    <>
      {/* One row: what the diff compares, its counts, then the view controls. */}
      <div
        class="flex min-h-(--cru-row-md) shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-b border-hairline px-3 py-1"
        data-testid="diff-toolbar"
      >
        <h2 class="sr-only">Diff</h2>
        {/* The label keeps room for itself: in a narrow pane, the controls
            move to a second row before the label shrinks to nothing. */}
        <div class="flex min-w-40 flex-1 items-center gap-2">
          <SourceLabel
            source={diffset.data?.source ?? props.source}
            proposal={props.proposal?.proposal()}
          />
          <ChangeCounts
            testId="diff-counts"
            class="shrink-0"
            added={totals().added}
            removed={totals().removed}
          />
          <ChatTarget session={props.session} />
        </div>
        <div class="ml-auto flex shrink-0 items-center gap-0.5">
          <div
            role="group"
            aria-label="Layout"
            class="flex items-center gap-0.5 rounded-md border border-hairline p-0.5"
          >
            <For each={[false, true]}>
              {(value) => (
                <button
                  type="button"
                  aria-pressed={split() === value}
                  aria-label={value ? 'Split' : 'Unified'}
                  title={value ? 'Split: base and current side by side' : 'Unified: one column'}
                  data-testid={`diff-layout-${value ? 'split' : 'unified'}`}
                  onClick={() => setSplit(value)}
                  class={`${iconButton} rounded-sm p-0.5 ${pressed(split() === value)} ${hit()}`}
                >
                  {value ? <Columns2 class="h-3.5 w-3.5" /> : <Rows2 class="h-3.5 w-3.5" />}
                </button>
              )}
            </For>
          </div>
          <button
            type="button"
            aria-pressed={wrap()}
            aria-label="Wrap"
            title="Wrap long lines"
            data-testid="diff-wrap"
            onClick={() => setWrap(!wrap())}
            class={`${iconButton} ${pressed(wrap())} ${hit()}`}
          >
            <WrapText class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            aria-label={hunksHidden() ? 'Expand all' : 'Collapse all'}
            title={hunksHidden() ? 'Expand all: show every hunk' : 'Collapse all: hide every hunk'}
            data-testid="diff-collapse-all"
            disabled={files().length === 0}
            onClick={toggleAllHunks}
            class={`${iconButton} ${hit()}`}
          >
            {hunksHidden() ? (
              <ChevronsUpDown class="h-3.5 w-3.5" />
            ) : (
              <ChevronsDownUp class="h-3.5 w-3.5" />
            )}
          </button>
          <button
            type="button"
            aria-label="Copy comments"
            title="Copy the open comments in the quickfix form"
            data-testid="diff-copy-comments"
            disabled={openComments().length === 0}
            onClick={copyComments}
            class={`${iconButton} ${hit()}`}
          >
            <MessageSquareText class="h-3.5 w-3.5" />
            <Show when={openComments().length > 0}>
              <span class="text-floor tabular-nums">{openComments().length}</span>
            </Show>
          </button>
          <button
            type="button"
            aria-label="Refresh"
            title="Refresh"
            data-testid="diff-refresh"
            disabled={diffset.isFetching}
            onClick={() => void invalidateDiffset(props.source)}
            class={`${iconButton} ${hit()}`}
          >
            <RefreshCw class={`h-3.5 w-3.5 ${diffset.isFetching ? 'animate-spin' : ''}`} />
          </button>
          <Show when={props.proposal}>
            {(controls) => <ProposalActions controls={controls()} />}
          </Show>
        </div>
      </div>

      <div ref={body} class="flex-1 overflow-y-auto">
        <UnreadableRoots roots={diffset.data?.unreadable_roots ?? []} />
        <Switch>
          <Match when={diffset.isError}>
            <p class="px-3 py-2 text-xs text-error" data-testid="diff-error">
              {diffset.error?.message}
            </p>
          </Match>
          <Match when={diffset.isPending}>
            <p class="px-3 py-2 text-xs text-muted-dark">Loading the diff…</p>
          </Match>
          <Match when={conflicts()}>
            {(files) => <ProposalConflicts files={files()} controls={props.proposal!} />}
          </Match>
          <Match when={files().length === 0}>
            <p class="px-3 py-2 text-xs text-muted-dark">No changes.</p>
          </Match>
          <Match when={true}>
            <For each={files()}>
              {(file) => (
                <FileSection
                  source={props.source}
                  session={props.session}
                  file={file}
                  comments={commentsOf(file)}
                  onResolve={resolve}
                  chips={chips}
                  expanded={isExpanded(file)}
                  onToggle={() => toggle(file)}
                  split={split()}
                  wrap={wrap()}
                  hunks={hunksOf(file)}
                  onHunkToggle={(label) => toggleHunk(file, label)}
                  decide={fileDecision()}
                />
              )}
            </For>
          </Match>
        </Switch>
      </div>
    </>
  );
};

interface FileSectionProps {
  source: DiffsetSource;
  /** The chat that a new comment attaches to. */
  session?: string;
  file: DiffFileEntry;
  /** The open comments of this file. */
  comments: ListedComment[];
  /** Marks a comment resolved. It rejects with the reason of a refusal. */
  onResolve: (commentId: string) => Promise<void>;
  /** The chip of each stored comment in the composer of the chat of the pane. */
  chips: ChipActions;
  expanded: boolean;
  onToggle: () => void;
  split: boolean;
  wrap: boolean;
  hunks: HunkChoice;
  onHunkToggle: (label: string) => void;
  /** The Accept and Reject of this file, for a proposal that takes them. */
  decide?: FileDecision;
}

interface FileDecision {
  busy: boolean;
  run: (kind: 'accept' | 'reject', file: DiffFileEntry) => void;
}

/** Why a file has no text, or null when it has text. */
function noTextReason(file: DiffFileEntry): string | null {
  if (file.binary) return 'Binary file. There is no text to show.';
  if (file.too_large) return 'File larger than 1 MiB. There is no text to show.';
  return null;
}

/** The status letter of a file, as `git status --short` writes it. */
const STATUS: Record<DiffFileEntry['status']['kind'], { letter: string; tone: string }> = {
  added: { letter: 'A', tone: 'text-ok' },
  modified: { letter: 'M', tone: 'text-muted' },
  deleted: { letter: 'D', tone: 'text-error' },
  renamed: { letter: 'R', tone: 'text-muted' },
};

const FileSection: Component<FileSectionProps> = (props) => {
  const renamedFrom = () => (props.file.status.kind === 'renamed' ? props.file.status.from : null);
  const copyPath = () =>
    void navigator.clipboard?.writeText(props.file.path).catch(() => undefined);
  const icon = () => fileIconFor(props.file.path);
  // The directory is quieter than the name, so the eye finds the file.
  const cut = () => props.file.path.lastIndexOf('/') + 1;
  const dir = () => props.file.path.slice(0, cut());
  const name = () => props.file.path.slice(cut());

  return (
    <section
      class="border-b border-hairline"
      data-testid={`diff-file-${fileKey(props.file)}`}
      data-file-key={fileKey(props.file)}
    >
      <div class="sticky top-0 z-10 flex h-(--cru-row-sm) items-center gap-1 bg-shell-bg pl-1.5 pr-2">
        <button
          type="button"
          aria-expanded={props.expanded}
          data-testid="diff-file-toggle"
          onClick={() => props.onToggle()}
          class={`flex min-w-0 items-center gap-1.5 rounded px-1 py-0.5 text-left hover:bg-hover-wash focus-ring ${hit()}`}
        >
          {props.expanded ? (
            <ChevronDown class="h-3.5 w-3.5 shrink-0 text-muted-dark" />
          ) : (
            <ChevronRight class="h-3.5 w-3.5 shrink-0 text-muted-dark" />
          )}
          <Dynamic
            component={icon().icon}
            class="h-3.5 w-3.5 shrink-0"
            style={{ color: icon().color }}
          />
          <span class="min-w-0 truncate text-xs" title={props.file.path}>
            <Show when={renamedFrom()}>
              {(from) => <span class="text-muted">{from()} → </span>}
            </Show>
            <span class="text-muted">{dir()}</span>
            <span class="text-shell-ink">{name()}</span>
          </span>
        </button>
        <button
          type="button"
          title="Copy the path"
          aria-label="Copy the path"
          data-testid="diff-file-copy"
          onClick={copyPath}
          class={`${iconButton} ${hit()}`}
        >
          <Copy class="h-3.5 w-3.5" />
        </button>
        {/* The rest of the row toggles the file too. The button above is the
            keyboard path, so this area stays out of the tab order. */}
        <div aria-hidden="true" class="h-full min-w-2 flex-1" onClick={() => props.onToggle()} />
        <span
          class={`w-3 shrink-0 text-center text-floor font-medium ${STATUS[props.file.status.kind].tone}`}
          title={props.file.status.kind}
          aria-label={props.file.status.kind}
          data-testid="diff-file-status"
        >
          {STATUS[props.file.status.kind].letter}
        </span>
        <ChangeCounts
          testId="diff-file-counts"
          class="shrink-0"
          added={props.file.added}
          removed={props.file.removed}
        />
        <Show when={props.decide}>
          {(decide) => (
            <div class="ml-1 flex shrink-0 items-center gap-1">
              <button
                type="button"
                title={`Reject ${props.file.path}. The file does not change.`}
                data-testid="proposal-reject-file"
                disabled={decide().busy}
                onClick={() => decide().run('reject', props.file)}
                class={`${decisionButton(false)} ${hit()}`}
              >
                Reject
              </button>
              <button
                type="button"
                title={`Write ${props.file.path}`}
                data-testid="proposal-accept-file"
                disabled={decide().busy}
                onClick={() => decide().run('accept', props.file)}
                class={`${decisionButton(true)} ${hit()}`}
              >
                Accept
              </button>
            </div>
          )}
        </Show>
      </div>
      <Show when={props.expanded}>
        <Show
          when={noTextReason(props.file)}
          fallback={
            <NearViewport>
              <FileBody
                source={props.source}
                session={props.session}
                file={props.file}
                comments={props.comments}
                onResolve={props.onResolve}
                chips={props.chips}
                split={props.split}
                wrap={props.wrap}
                hunks={props.hunks}
                onHunkToggle={props.onHunkToggle}
              />
            </NearViewport>
          }
        >
          {(reason) => <p class="px-3 pb-2 text-xs text-muted-dark">{reason()}</p>}
        </Show>
        <EndComments
          comments={props.comments}
          split={props.split}
          onResolve={props.onResolve}
          chips={props.chips}
          hasChat={!!props.session}
        />
      </Show>
    </section>
  );
};

/**
 * Mounts its children when the placeholder comes near the viewport.
 *
 * An expanded section asks for its text only when the user can see it soon.
 * Where the browser has no `IntersectionObserver`, the children mount at once.
 */
const NearViewport: Component<{ children: JSX.Element }> = (props) => {
  const [near, setNear] = createSignal(typeof IntersectionObserver === 'undefined');
  const watch = (el: HTMLDivElement) => {
    if (near()) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setNear(true);
          observer.disconnect();
        }
      },
      { rootMargin: '400px 0px' },
    );
    observer.observe(el);
    onCleanup(() => observer.disconnect());
  };
  return (
    <Show when={near()} fallback={<div ref={watch} class="h-24" />}>
      {props.children}
    </Show>
  );
};

/**
 * The comments that the editor cannot show under a line, at the end of the
 * file: an outdated comment, and a comment on the base side in the unified
 * view, which shows only the line numbers of the current side.
 */
const EndComments: Component<{
  comments: ListedComment[];
  split: boolean;
  onResolve: (commentId: string) => Promise<void>;
  chips: ChipActions;
  /** The pane has a chat, so a comment can take a chip. */
  hasChat: boolean;
}> = (props) => {
  const rows = () =>
    props.comments.filter((l) => l.outdated || (!props.split && l.comment.side === 'base'));
  return (
    <Show when={rows().length > 0}>
      <ul class="flex flex-col gap-1 px-3 pb-2">
        <For each={rows()}>
          {(listed) => (
            <li
              data-testid={listed.outdated ? 'diff-comment-outdated' : 'diff-comment-base'}
              class="border-l-2 border-hairline px-2 py-1 text-xs"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="text-floor text-muted-dark">
                  {listed.outdated ? 'Outdated' : 'Base side'} ·{' '}
                  {spanLabel({
                    first: listed.comment.line_range.start,
                    last: listed.comment.line_range.end - 1,
                  })}
                </span>
                <span class="flex shrink-0 items-center gap-2">
                  <Show when={props.hasChat}>
                    <Show
                      when={!props.chips.attached(listed.comment.id)}
                      fallback={
                        <span
                          class="text-floor text-muted-dark"
                          data-testid="diff-comment-attached"
                        >
                          In the composer
                        </span>
                      }
                    >
                      <button
                        type="button"
                        data-testid="diff-comment-attach"
                        title="Put this comment in the composer of this chat again"
                        onClick={() => props.chips.attach(listed.comment)}
                        class={endCommentButton}
                      >
                        Attach
                      </button>
                    </Show>
                  </Show>
                  <ResolveButton onResolve={() => props.onResolve(listed.comment.id)} />
                </span>
              </div>
              <p class="whitespace-pre-wrap text-shell-ink">{listed.comment.body}</p>
            </li>
          )}
        </For>
      </ul>
    </Show>
  );
};

/** A small control on a comment row at the end of a file. */
const endCommentButton =
  'rounded border border-hairline px-1.5 text-floor text-muted hover:bg-hover-wash hover:text-shell-ink disabled:opacity-50 focus-ring';

/** Resolve on a comment at the end of a file. A refusal shows its reason. */
const ResolveButton: Component<{ onResolve: () => Promise<void> }> = (props) => {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const run = () => {
    setBusy(true);
    setError(null);
    props.onResolve().catch((err: unknown) => {
      setError(err instanceof Error ? err.message : 'The comment was not resolved');
      setBusy(false);
    });
  };
  return (
    <span class="flex shrink-0 items-center gap-2">
      <Show when={error()}>
        {(message) => <span class="text-floor text-error">{message()}</span>}
      </Show>
      <button
        type="button"
        data-testid="diff-comment-resolve"
        title="Mark this comment resolved. It leaves the open comments."
        disabled={busy()}
        onClick={run}
        class={endCommentButton}
      >
        Resolve
      </button>
    </span>
  );
};

interface FileBodyProps {
  source: DiffsetSource;
  /** The chat that a new comment attaches to. */
  session?: string;
  file: DiffFileEntry;
  comments: ListedComment[];
  onResolve: (commentId: string) => Promise<void>;
  chips: ChipActions;
  split: boolean;
  wrap: boolean;
  hunks: HunkChoice;
  onHunkToggle: (label: string) => void;
}

const FileBody: Component<FileBodyProps> = (props) => {
  const text = useDiffFile(
    () => props.source,
    () => props.file,
  );
  const post = usePostDiffComment();

  const comment = async (side: CommentSide, span: LineSpan, body: string) => {
    const file = props.file;
    const source = props.source;
    const stored = await post.mutateAsync({
      source,
      // A branch source names its own root. A session record and a proposal
      // need the root of the file.
      ...(source.kind === 'branch' ? {} : { root: file.root }),
      path: file.path,
      ...(file.status.kind === 'renamed' ? { from: file.status.from } : {}),
      line_start: span.first,
      line_end: span.last + 1,
      side,
      body,
    });
    // The comment is stored. The chat of this pane, when it has one, carries
    // the reference until the user sends a message.
    props.chips.attach(stored);
  };
  const host = (side: CommentSide): CommentHost => ({
    side,
    comment: (span, body) => comment(side, span, body),
    chat: () => props.session ?? null,
    resolve: (commentId) => props.onResolve(commentId),
    attached: (commentId) => props.chips.attached(commentId),
    attach: (stored) => props.chips.attach(stored),
  });
  const hosts = { base: host('base'), current: host('current') };

  // The comments that the editor shows under their lines.
  const placed = () => props.comments.filter((l) => !l.outdated).map((l) => l.comment);

  return (
    <Switch>
      <Match when={text.isError}>
        <p class="px-3 pb-2 text-xs text-error">{text.error?.message}</p>
      </Match>
      <Match when={text.data}>
        {(data) => (
          <FileEditor
            path={props.file.path}
            text={data()}
            split={props.split}
            wrap={props.wrap}
            hosts={hosts}
            comments={placed()}
            hunks={props.hunks}
            onHunkToggle={props.onHunkToggle}
          />
        )}
      </Match>
      <Match when={true}>
        <p class="px-3 pb-2 text-xs text-muted-dark">Loading {props.file.path}…</p>
      </Match>
    </Switch>
  );
};

interface FileEditorProps {
  path: string;
  text: DiffFileText;
  split: boolean;
  wrap: boolean;
  hosts: Record<CommentSide, CommentHost>;
  comments: DiffComment[];
  hunks: HunkChoice;
  onHunkToggle: (label: string) => void;
}

/** The test on a hunk label that `setHiddenHunks` takes. */
const hiddenTest =
  (choice: HunkChoice) =>
  (label: string): boolean =>
    choice.own[label] ?? choice.hidden;

/** The read-only CodeMirror view of one file: unified, or split in two. */
const FileEditor: Component<FileEditorProps> = (props) => {
  let host!: HTMLDivElement;
  let destroy: (() => void) | undefined;
  // Each editor with the side that its line numbers count on.
  const [views, setViews] = createSignal<{ view: EditorView; side: CommentSide }[]>([]);

  createEffect(() => {
    destroy?.();
    const original = props.text.base_text ?? '';
    const current = props.text.current_text ?? '';
    const setup = {
      original,
      path: props.path,
      wrap: props.wrap,
      collapse: COLLAPSE,
      hideFinalNewline: hidesFinalNewline(original, current),
      hunks: { current, onToggle: (label: string) => props.onHunkToggle(label) },
    };
    const hosts = untrack(() => props.hosts);
    const extra = (side: CommentSide): Extension[] => [...commentExtensions(hosts[side])];
    if (props.split) {
      const view = new MergeView({
        a: {
          doc: original,
          extensions: [...mergeViewExtensions({ ...setup, split: true }), ...extra('base')],
        },
        b: {
          doc: current,
          extensions: [...mergeViewExtensions({ ...setup, split: true }), ...extra('current')],
        },
        parent: host,
        highlightChanges: true,
        gutter: true,
        collapseUnchanged: COLLAPSE,
      });
      setViews([
        { view: view.a, side: 'base' },
        { view: view.b, side: 'current' },
      ]);
      destroy = () => view.destroy();
    } else {
      // The unified view shows the line numbers of the current side only.
      const view = new EditorView({
        state: EditorState.create({
          doc: current,
          extensions: [...mergeViewExtensions(setup), ...extra('current')],
        }),
        parent: host,
      });
      setViews([{ view, side: 'current' }]);
      destroy = () => view.destroy();
    }
  });

  // After the editors, so that a new editor gets the hidden hunks at once.
  createEffect(() => {
    const hidden = hiddenTest(props.hunks);
    for (const { view } of views()) view.dispatch({ effects: setHiddenHunks.of(hidden) });
  });

  createEffect(() => {
    const comments = props.comments;
    for (const { view, side } of views()) {
      view.dispatch({ effects: setComments.of(comments.filter((c) => c.side === side)) });
    }
  });

  onCleanup(() => {
    destroy?.();
    destroy = undefined;
  });

  return <div ref={host} class="font-mono text-xs" data-testid="diff-file-editor" />;
};
