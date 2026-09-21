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
  type JSX,
} from 'solid-js';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, lineNumbers } from '@codemirror/view';
import { MergeView } from '@codemirror/merge';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import { mergeViewExtensions, type MergeCollapse } from '@/lib/merge-view';
import { diffsetLabel, type DiffFileEntry, type DiffFileText, type DiffsetSource } from '@/lib/diffset';
import { invalidateDiffset, useDiffFile, useDiffset } from '@/lib/query/diff';
import { ChevronDown, ChevronRight, ChevronsDownUp, Copy, RefreshCw } from '@/lib/icons';
import { hit } from '@/lib/touch';

/** A file with more changed lines than this starts collapsed. */
export const LARGE_FILE_LINES = 400;

/** The unchanged-line collapse of the design: 3 lines of context, 4 at least. */
const COLLAPSE: MergeCollapse = { margin: 3, minSize: 4 };

/**
 * The change bar in the theme colors: green for an added line, red for a
 * removed one. In a split view the left editor is the removed side.
 */
const barTheme = EditorView.theme({
  '.cm-changedLineGutter': { background: 'var(--color-ok)' },
  '.cm-deletedLineGutter': { background: 'var(--color-error)' },
  '&.cm-merge-a .cm-changedLineGutter': { background: 'var(--color-error)' },
});

export interface DiffPanelProps {
  /** The tab metadata that `openDiff` writes. */
  source?: DiffsetSource;
}

const toolButton =
  'rounded border border-hairline px-2 py-0.5 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50';

export const DiffPanel: Component<DiffPanelProps> = (props) => {
  return (
    <PanelShell>
      <Show
        when={props.source}
        fallback={<p class="p-3 text-xs text-muted-dark">This tab names no diff.</p>}
      >
        {(source) => <DiffsetView source={source()} />}
      </Show>
    </PanelShell>
  );
};

const DiffsetView: Component<{ source: DiffsetSource }> = (props) => {
  const diffset = useDiffset(() => props.source);
  const [split, setSplit] = createSignal(false);
  const [wrap, setWrap] = createSignal(true);
  // The choice of the user for each path. A path without a choice takes the
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
    expanded()[file.path] ?? file.added + file.removed <= LARGE_FILE_LINES;
  const toggle = (file: DiffFileEntry) =>
    setExpanded((prev) => ({ ...prev, [file.path]: !isExpanded(file) }));
  const collapseAll = () =>
    setExpanded(Object.fromEntries(files().map((f) => [f.path, false])));

  return (
    <>
      <PanelHeader title="Diff" class="shrink-0">
        <div class="mt-1.5 flex flex-wrap items-center gap-2">
          <span class="min-w-0 truncate text-xs font-mono text-shell-ink" data-testid="diff-source">
            {diffsetLabel(diffset.data?.source ?? props.source)}
          </span>
          <span class="text-floor font-mono text-muted-dark" data-testid="diff-counts">
            +{totals().added} −{totals().removed}
          </span>
        </div>
        <div class="mt-1 flex flex-wrap items-center gap-1.5">
          <div role="group" aria-label="Layout" class="flex items-center rounded border border-hairline text-floor">
            <For each={[false, true]}>
              {(value) => (
                <button
                  type="button"
                  aria-pressed={split() === value}
                  data-testid={`diff-layout-${value ? 'split' : 'unified'}`}
                  onClick={() => setSplit(value)}
                  class={`px-2 py-0.5 hover:bg-hover-wash ${
                    split() === value ? 'bg-hover-wash text-shell-ink' : 'text-muted-dark hover:text-shell-ink'
                  } ${hit()}`}
                >
                  {value ? 'Split' : 'Unified'}
                </button>
              )}
            </For>
          </div>
          <button
            type="button"
            aria-pressed={wrap()}
            data-testid="diff-wrap"
            onClick={() => setWrap(!wrap())}
            class={`${toolButton} ${wrap() ? 'text-shell-ink' : ''} ${hit()}`}
          >
            Wrap
          </button>
          <button
            type="button"
            title="Collapse all"
            data-testid="diff-collapse-all"
            disabled={files().length === 0}
            onClick={collapseAll}
            class={`${toolButton} flex items-center gap-1 ${hit()}`}
          >
            <ChevronsDownUp class="w-3.5 h-3.5" />
            Collapse all
          </button>
          <button
            type="button"
            title="Refresh"
            data-testid="diff-refresh"
            disabled={diffset.isFetching}
            onClick={() => void invalidateDiffset(props.source)}
            class={`ml-auto rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50 ${hit()}`}
          >
            <RefreshCw class={`w-3.5 h-3.5 ${diffset.isFetching ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </PanelHeader>

      <div class="flex-1 overflow-y-auto">
        <Switch>
          <Match when={diffset.isError}>
            <p class="px-3 py-2 text-xs text-error" data-testid="diff-error">
              {diffset.error?.message}
            </p>
          </Match>
          <Match when={diffset.isPending}>
            <p class="px-3 py-2 text-xs text-muted-dark">Loading the diff…</p>
          </Match>
          <Match when={files().length === 0}>
            <p class="px-3 py-2 text-xs text-muted-dark">No changes.</p>
          </Match>
          <Match when={true}>
            <For each={files()}>
              {(file) => (
                <FileSection
                  source={props.source}
                  file={file}
                  expanded={isExpanded(file)}
                  onToggle={() => toggle(file)}
                  split={split()}
                  wrap={wrap()}
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
  file: DiffFileEntry;
  expanded: boolean;
  onToggle: () => void;
  split: boolean;
  wrap: boolean;
}

/** Why a file has no text, or null when it has text. */
function noTextReason(file: DiffFileEntry): string | null {
  if (file.binary) return 'Binary file. There is no text to show.';
  if (file.too_large) return 'File larger than 1 MiB. There is no text to show.';
  return null;
}

const FileSection: Component<FileSectionProps> = (props) => {
  const renamedFrom = () => (props.file.status.kind === 'renamed' ? props.file.status.from : null);
  const copyPath = () => void navigator.clipboard?.writeText(props.file.path).catch(() => undefined);

  return (
    <section class="border-b border-hairline" data-testid={`diff-file-${props.file.path}`}>
      <div class="sticky top-0 z-10 flex items-center gap-1.5 bg-shell-bg px-2 py-1">
        <button
          type="button"
          aria-expanded={props.expanded}
          data-testid="diff-file-toggle"
          onClick={() => props.onToggle()}
          class={`flex min-w-0 flex-1 items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-hover-wash ${hit()}`}
        >
          {props.expanded ? (
            <ChevronDown class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
          ) : (
            <ChevronRight class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
          )}
          <span class="min-w-0 truncate text-xs font-mono text-shell-ink" title={props.file.path}>
            <Show when={renamedFrom()}>{(from) => <span class="text-muted-dark">{from()} → </span>}</Show>
            {props.file.path}
          </span>
        </button>
        <span class="shrink-0 text-floor text-muted-dark">{props.file.status.kind}</span>
        <button
          type="button"
          title="Copy the path"
          data-testid="diff-file-copy"
          onClick={copyPath}
          class={`rounded p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash ${hit()}`}
        >
          <Copy class="w-3.5 h-3.5" />
        </button>
        <span class="shrink-0 text-floor font-mono text-muted-dark" data-testid="diff-file-counts">
          +{props.file.added} −{props.file.removed}
        </span>
      </div>
      <Show when={props.expanded}>
        <Show
          when={noTextReason(props.file)}
          fallback={
            <NearViewport>
              <FileBody source={props.source} file={props.file} split={props.split} wrap={props.wrap} />
            </NearViewport>
          }
        >
          {(reason) => <p class="px-3 pb-2 text-xs text-muted-dark">{reason()}</p>}
        </Show>
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

const FileBody: Component<{ source: DiffsetSource; file: DiffFileEntry; split: boolean; wrap: boolean }> = (
  props,
) => {
  const text = useDiffFile(
    () => props.source,
    () => props.file,
  );
  return (
    <Switch>
      <Match when={text.isError}>
        <p class="px-3 pb-2 text-xs text-error">{text.error?.message}</p>
      </Match>
      <Match when={text.data}>
        {(data) => <FileEditor path={props.file.path} text={data()} split={props.split} wrap={props.wrap} />}
      </Match>
      <Match when={true}>
        <p class="px-3 pb-2 text-xs text-muted-dark">Loading {props.file.path}…</p>
      </Match>
    </Switch>
  );
};

/** The read-only CodeMirror view of one file: unified, or split in two. */
const FileEditor: Component<{ path: string; text: DiffFileText; split: boolean; wrap: boolean }> = (props) => {
  let host!: HTMLDivElement;
  let destroy: (() => void) | undefined;

  createEffect(() => {
    destroy?.();
    const original = props.text.base_text ?? '';
    const current = props.text.current_text ?? '';
    const setup = { original, path: props.path, wrap: props.wrap, collapse: COLLAPSE };
    const extra: Extension[] = [lineNumbers(), barTheme];
    if (props.split) {
      const side = [...mergeViewExtensions({ ...setup, split: true }), ...extra];
      const view = new MergeView({
        a: { doc: original, extensions: side },
        b: { doc: current, extensions: side },
        parent: host,
        highlightChanges: true,
        gutter: true,
        collapseUnchanged: COLLAPSE,
      });
      destroy = () => view.destroy();
    } else {
      const view = new EditorView({
        state: EditorState.create({ doc: current, extensions: [...mergeViewExtensions(setup), ...extra] }),
        parent: host,
      });
      destroy = () => view.destroy();
    }
  });

  onCleanup(() => {
    destroy?.();
    destroy = undefined;
  });

  return <div ref={host} class="font-mono text-xs" data-testid="diff-file-editor" />;
};
