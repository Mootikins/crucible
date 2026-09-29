/**
 * The mockup's tab bodies. They carry the content decisions of the design
 * rounds; the windowing core draws every frame, rail, tab and split around
 * them.
 */
import { For, Show, createMemo, createSignal, type Component, type JSX } from 'solid-js';
import {
  ArrowUp,
  BookOpen,
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  Code,
  Eye,
  GitCompare,
  Link,
  Maximize2,
  MessageSquare,
  Mic,
  Minimize2,
  MoreHorizontal,
  Pencil,
  Plus,
  Search,
  Sparkles,
  X,
  ChevronLeft,
} from 'lucide-solid';
import { renderMarkdown } from '@/lib/markdown';
import { windowActions, windowStore } from '@/windowing/store';
import { KILN_PATHS, PROJECT_PATHS } from './data';
import {
  answerPermission,
  decide,
  focusedNote,
  hunkLines,
  pendingHunks,
  removeQueued,
  send,
  sendQueuedNow,
  setState,
  state,
  type Item,
} from './state';
import { canGoBack, canGoForward, goHistory, hoverEnd, hoverStart, openChanges, openNote, openSession, whereFor } from './actions';

const basename = (p: string) => p.split('/').pop() ?? p;

/** Run a layout change as a view transition, so an expand morphs instead of jumping. */
export function withTransition(change: () => void) {
  const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
  if (doc.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches) doc.startViewTransition(change);
  else change();
}
const resolve = (target: string): string | null => {
  const t = target.split('#')[0]!.trim().replace(/\.md$/, '');
  if (KILN_PATHS.includes(t)) return t;
  return KILN_PATHS.find((p) => basename(p).toLowerCase() === basename(t).toLowerCase()) ?? null;
};

/**
 * Wikilinks inside rendered markdown: a click opens the note, a hover floats
 * it. Event delegation, because the markdown arrives as HTML.
 */
function linkHandlers(fromSession: boolean): { onClick: (e: MouseEvent) => void; onMouseOver: (e: MouseEvent) => void } {
  const link = (e: Event) => (e.target as HTMLElement).closest<HTMLElement>('[data-note]');
  return {
    onClick: (e) => {
      const a = link(e);
      if (!a) return;
      e.preventDefault();
      const path = resolve(a.dataset.note ?? '');
      if (path) openNote(path, { fromSession, where: whereFor(e) });
    },
    onMouseOver: (e) => {
      const a = link(e);
      const path = a && resolve(a.dataset.note ?? '');
      if (a && path) {
        hoverStart(a, path);
        a.addEventListener('mouseleave', hoverEnd, { once: true });
      }
    },
  };
}

// ======================================================================= sessions
const Mark: Component<{ sid: string }> = (props) => {
  const s = () => state.sessions[props.sid]!;
  return (
    <Show
      when={s().status !== 'idle'}
      fallback={<span class="mk-ident" style={{ background: s().color, opacity: 0.55 }} />}
    >
      <span class={`mk-mark-${s().status}`} title={{ need: 'Needs you', run: 'Working', owe: 'Changes to review', idle: '' }[s().status]} />
    </Show>
  );
};

const statusText = (sid: string) => {
  const s = state.sessions[sid]!;
  if (s.status === 'need') return { text: 'Needs you', cls: 'attn' };
  if (s.status === 'run') return { text: 'Working', cls: 'run' };
  if (s.status === 'owe') return { text: `${pendingHunks(sid).length} to review`, cls: 'attn' };
  return { text: s.time, cls: '' };
};

export const SessionsPanel: Component = () => {
  const groups = createMemo(() => {
    const out: Record<string, string[]> = {};
    for (const [sid, s] of Object.entries(state.sessions)) (out[s.group] ??= []).push(sid);
    return Object.entries(out);
  });
  return (
    <div class="mk-scroll mk-list">
      <For each={groups()}>
        {([group, sids]) => (
          <>
            <div class="mk-grouplabel">{group}</div>
            <For each={sids}>
              {(sid) => (
                <button
                  type="button"
                  class="mk-srow"
                  aria-current={state.active === sid}
                  title={state.sessions[sid]!.title}
                  onClick={() => openSession(sid)}
                >
                  <Mark sid={sid} />
                  <span class="mk-t">{state.sessions[sid]!.title}</span>
                  <span class={`mk-meta ${statusText(sid).cls}`}>{statusText(sid).text}</span>
                </button>
              )}
            </For>
          </>
        )}
      </For>
      <button type="button" class="mk-srow mk-more">
        <span />
        <span class="mk-t">All sessions</span>
        <span />
      </button>
    </div>
  );
};

// ========================================================================= files
interface TreeNode {
  name: string;
  path: string;
  dirs: Map<string, TreeNode>;
  files: string[];
}
function buildTree(paths: readonly string[]): TreeNode {
  const root: TreeNode = { name: '', path: '', dirs: new Map(), files: [] };
  for (const p of paths) {
    const parts = p.split('/');
    let n = root;
    parts.slice(0, -1).forEach((part, k) => {
      if (!n.dirs.has(part)) n.dirs.set(part, { name: part, path: parts.slice(0, k + 1).join('/'), dirs: new Map(), files: [] });
      n = n.dirs.get(part)!;
    });
    n.files.push(p);
  }
  return root;
}
const KILN_TREE = buildTree(KILN_PATHS);
const TOUCHED: Record<string, string[]> = {
  s1: ['Help/Concepts/Precognition', 'Help/Concepts/Semantic Search'],
  s2: ['Help/Concepts/Review Ledger'],
  s3: ['Help/Concepts/Kilns', 'Help/Tags', 'Organization Styles/Zettelkasten', 'Help/Concepts/Session Compaction'],
};

export const FilesPanel: Component = () => {
  const [open, setOpen] = createSignal(new Set(['docs', 'docs:Help', 'docs:Help/Concepts']));
  const toggle = (key: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });
  const session = () => state.sessions[state.active]!;
  const activePath = () => {
    const id = windowStore.activePaneId ? windowActions.getPaneTabGroupId(windowStore.activePaneId) : null;
    const g = id ? windowStore.tabGroups[id] : undefined;
    return g?.tabs.find((t) => t.id === g.activeTabId)?.metadata?.path as string | undefined;
  };

  const Rows: Component<{ node: TreeNode; depth: number }> = (p) => (
    <>
      <For each={[...p.node.dirs.values()].sort((a, b) => a.name.localeCompare(b.name))}>
        {(d) => (
          <>
            <button type="button" class="mk-trow mk-dir" style={{ 'padding-left': `${4 + p.depth * 14}px` }} onClick={() => toggle(`docs:${d.path}`)}>
              <Show when={open().has(`docs:${d.path}`)} fallback={<ChevronRight class="mk-i" />}>
                <ChevronDown class="mk-i" />
              </Show>
              <span class="mk-t">{d.name}</span>
            </button>
            <Show when={open().has(`docs:${d.path}`)}>
              <Rows node={d} depth={p.depth + 1} />
            </Show>
          </>
        )}
      </For>
      <For each={[...p.node.files].sort((a, b) => basename(a).localeCompare(basename(b)))}>
        {(f) => {
          const owe = () => pendingHunks(state.active).filter((id) => state.hunks[id]!.path === f).length;
          const touched = () => (TOUCHED[state.active] ?? []).includes(f);
          return (
            <button
              type="button"
              class="mk-trow"
              style={{ 'padding-left': `${18 + p.depth * 14}px` }}
              aria-current={activePath() === f ? 'page' : undefined}
              onClick={(e) => openNote(f, { where: whereFor(e) })}
            >
              <span class="mk-t">{basename(f)}</span>
              <Show when={owe()} fallback={<Show when={touched()}><span class="mk-touch" style={{ background: session().color }} title="Used by this session" /></Show>}>
                <span class="mk-owe" title={`${owe()} to review`}>{owe()}</span>
              </Show>
            </button>
          );
        }}
      </For>
    </>
  );

  const Root: Component<{ key: 'docs' | 'crucible' | 'folder'; label: string; kind: string; children: JSX.Element }> = (p) => {
    const mine = () => session().roots.includes(p.key);
    return (
      <>
        <button type="button" class="mk-rootrow" classList={{ dim: !mine() }} onClick={() => toggle(p.key)}>
          <Show when={open().has(p.key)} fallback={<ChevronRight class="mk-i" />}>
            <ChevronDown class="mk-i" />
          </Show>
          <span>{p.label}</span>
          <span class="mk-kind">{p.kind}</span>
          <span class="mk-grow" />
          <Show when={mine()}>
            <span class="mk-sessionbar" style={{ background: session().color }} title="A root of the active session" />
          </Show>
        </button>
        <Show when={open().has(p.key)}>{p.children}</Show>
      </>
    );
  };

  return (
    <div class="mk-scroll mk-tree">
      <Root key="docs" label="docs" kind="kiln">
        <Rows node={KILN_TREE} depth={0} />
      </Root>
      <Root key="crucible" label="crucible" kind="project">
        <For each={PROJECT_PATHS}>
          {(p) => (
            <div class="mk-trow" classList={{ 'mk-dir': p.endsWith('/') }} style={{ 'padding-left': p.endsWith('/') ? '4px' : '18px' }}>
              <Show when={p.endsWith('/')}>
                <ChevronRight class="mk-i" />
              </Show>
              <span class="mk-t">{p.replace(/\/$/, '')}</span>
            </div>
          )}
        </For>
      </Root>
      <Show when={session().roots.includes('folder')}>
        <Root key="folder" label="Session folder" kind="workspace">
          <div class="mk-trow mk-quiet" style={{ 'padding-left': '18px' }}>
            <span class="mk-t">No files yet</span>
          </div>
        </Root>
      </Show>
    </div>
  );
};

// ========================================================================== note
function parseFront(src: string): [Record<string, string | string[]>, string] {
  if (!src.startsWith('---\n')) return [{}, src];
  const end = src.indexOf('\n---', 4);
  const props: Record<string, string | string[]> = {};
  for (const line of src.slice(4, end).split('\n')) {
    const m = line.match(/^(\w+):\s*(.*)$/);
    if (m) props[m[1]!] = m[2]!;
  }
  if (typeof props.tags === 'string') props.tags = props.tags.replace(/^\[|\]$/g, '').split(',').map((t) => t.trim()).filter(Boolean);
  return [props, src.slice(end + 4).replace(/^\n/, '')];
}

type Segment = { kind: 'md'; text: string } | { kind: 'hunk'; id: string };
function segments(body: string): Segment[] {
  const out: Segment[] = [];
  const re = /:::hunk (\w+)\n[\s\S]*?\n:::\n?/g;
  let at = 0;
  for (const m of body.matchAll(re)) {
    out.push({ kind: 'md', text: body.slice(at, m.index) });
    out.push({ kind: 'hunk', id: m[1]! });
    at = m.index! + m[0].length;
  }
  out.push({ kind: 'md', text: body.slice(at) });
  return out;
}

/** One agent edit, inline in the note: who made it, then the review buttons, then the lines. */
const HunkBlock: Component<{ id: string }> = (props) => {
  const h = () => state.hunks[props.id]!;
  const s = () => state.sessions[h().session]!;
  const lines = () => hunkLines(props.id);
  const [arm, setArm] = createSignal(false);
  return (
    <Show when={h().state === 'pending'}>
      <div class="mk-hunk" data-hunk={props.id}>
        <div class="mk-hhead">
          <span class="mk-ident" style={{ background: s().color }} />
          <span class="mk-who">{s().title}</span>
          <span class="mk-grow" />
          <Show when={!h().external}>
            <button
              type="button"
              class="mk-btn sm ghost danger"
              onClick={() => {
                // A reject reverts on disk, so it asks once, in place.
                if (!arm()) {
                  setArm(true);
                  setTimeout(() => setArm(false), 3500);
                  return;
                }
                decide([props.id], false);
              }}
            >
              {arm() ? 'Revert on disk?' : 'Reject'}
            </button>
            <button type="button" class="mk-btn sm primary" onClick={() => decide([props.id], true)}>
              Accept
            </button>
          </Show>
        </div>
        <Show when={lines().del.length}>
          <div class="mk-del" innerHTML={renderMarkdown(lines().del.join('\n'))} />
        </Show>
        <Show when={lines().add.length}>
          <div class="mk-add" innerHTML={renderMarkdown(lines().add.join('\n'))} />
        </Show>
      </div>
    </Show>
  );
};

export const NoteView: Component<{ tabId?: string; path: string }> = (props) => {
  const [mode, setMode] = createSignal<'live' | 'source' | 'read'>('live');
  const src = () => state.notes[props.path];
  const parsed = createMemo(() => (src() ? parseFront(src()!) : null));
  const parts = () => props.path.split('/');
  const go = (step: -1 | 1) => props.tabId && goHistory(props.tabId, step);
  return (
    <div
      class="mk-body"
      // Alt+Left/Right and the mouse's back/forward buttons, as in a browser.
      onKeyDown={(e) => {
        if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
          e.preventDefault();
          go(e.key === 'ArrowLeft' ? -1 : 1);
        }
      }}
      onMouseUp={(e) => {
        if (e.button === 3 || e.button === 4) go(e.button === 3 ? -1 : 1);
      }}
    >
      <div class="mk-crumbbar">
        <Show when={props.tabId}>
          {(id) => (
            <div class="mk-nav">
              <button type="button" class="mk-iconbtn" title="Back (Alt+Left)" disabled={!canGoBack(id())} onClick={() => go(-1)}>
                <ChevronLeft class="mk-i" />
              </button>
              <button type="button" class="mk-iconbtn" title="Forward (Alt+Right)" disabled={!canGoForward(id())} onClick={() => go(1)}>
                <ChevronRight class="mk-i" />
              </button>
            </div>
          )}
        </Show>
        <span class="mk-path">
          docs
          <For each={parts()}>
            {(p, i) => (
              <>
                <span class="mk-sep">/</span>
                {i() === parts().length - 1 ? <b>{p}</b> : p}
              </>
            )}
          </For>
        </span>
        <div class="mk-seg" role="group" aria-label="View">
          <button type="button" aria-pressed={mode() === 'live'} title="Live preview" onClick={() => setMode('live')}><Pencil class="mk-i" /></button>
          <button type="button" aria-pressed={mode() === 'source'} title="Source" onClick={() => setMode('source')}><Code class="mk-i" /></button>
          <button type="button" aria-pressed={mode() === 'read'} title="Reading view" onClick={() => setMode('read')}><BookOpen class="mk-i" /></button>
        </div>
        <button type="button" class="mk-iconbtn" title="Ask the session about this note" onClick={() => document.querySelector<HTMLTextAreaElement>('.mk-composer textarea')?.focus()}>
          <MessageSquare class="mk-i" />
        </button>
      </div>
      <div class="mk-scroll">
        <Show
          when={parsed()}
          fallback={
            <article class="mk-note">
              <h1>{basename(props.path)}</h1>
              <p class="mk-quiet">The mockup carries the text of six notes from the docs kiln. The real app reads {props.path}.md from the daemon.</p>
              <For each={pendingHunks().filter((id) => state.hunks[id]!.path === props.path)}>{(id) => <HunkBlock id={id} />}</For>
            </article>
          }
        >
          {(p) => (
            <Show when={mode() !== 'source'} fallback={<pre class="mk-source">{src()}</pre>}>
              <article class="mk-note" {...linkHandlers(false)}>
                <Show when={Object.keys(p()[0]).length}>
                  <dl class="mk-props">
                    <Show when={p()[0].description}><dt>description</dt><dd>{p()[0].description as string}</dd></Show>
                    <Show when={p()[0].status}><dt>status</dt><dd class="mk-status">{p()[0].status as string}</dd></Show>
                    <Show when={p()[0].tags}>
                      <dt>tags</dt>
                      <dd><For each={p()[0].tags as string[]}>{(t) => <span class="mk-tag">#{t}</span>}</For></dd>
                    </Show>
                  </dl>
                </Show>
                <For each={segments(p()[1])}>
                  {(seg) => (seg.kind === 'md' ? <div innerHTML={renderMarkdown(seg.text)} /> : <HunkBlock id={seg.id} />)}
                </For>
              </article>
            </Show>
          )}
        </Show>
      </div>
    </div>
  );
};

// ======================================================================= changes
/** The review: several files, hunk by hunk, with the review buttons on top of each hunk. */
export const ChangesView: Component<{ sid: string }> = (props) => {
  const ids = () => Object.entries(state.hunks).filter(([, h]) => h.session === props.sid && h.state !== 'absent').map(([id]) => id);
  const byFile = createMemo(() => {
    const out: Record<string, string[]> = {};
    for (const id of ids()) (out[state.hunks[id]!.path] ??= []).push(id);
    return Object.entries(out);
  });
  const owed = () => pendingHunks(props.sid);
  return (
    <div class="mk-scroll">
      <div class="mk-changes">
        <header>
          <h2>
            Changes
            <small>
              <span class="mk-ident" style={{ background: state.sessions[props.sid]!.color }} />
              {state.sessions[props.sid]!.title}
            </small>
          </h2>
          <button type="button" class="mk-btn sm ghost danger" disabled={!owed().length} onClick={() => decide(owed(), false)}>Reject all</button>
          <button type="button" class="mk-btn sm primary" disabled={!owed().length} onClick={() => decide(owed(), true)}>
            <Check class="mk-i" />Accept all
          </button>
        </header>
        <For each={byFile()} fallback={<p class="mk-quiet">Nothing to review.</p>}>
          {([path, hs]) => (
            <section class="mk-cfile">
              <button type="button" class="mk-fname" onClick={() => openNote(path)}>{path}.md</button>
              <For each={hs}>
                {(id) => {
                  const h = () => state.hunks[id]!;
                  return (
                    <div class="mk-chunk" classList={{ done: h().state !== 'pending' }}>
                      <div class="mk-chead">
                        <Show when={h().external} fallback={<span class="mk-quiet">{h().state === 'pending' ? '' : h().state === 'accepted' ? 'Accepted' : 'Rejected'}</span>}>
                          <span class="mk-state">Your edit</span>
                        </Show>
                        <span class="mk-grow" />
                        <Show when={h().state === 'pending' && !h().external}>
                          <button type="button" class="mk-btn sm ghost danger" onClick={() => decide([id], false)}>Reject</button>
                          <button type="button" class="mk-btn sm" onClick={() => decide([id], true)}><Check class="mk-i" />Accept</button>
                        </Show>
                      </div>
                      <pre class="mk-mini">
                        <For each={hunkLines(id).del}>{(l) => <span class="d">{l || ' '}</span>}</For>
                        <For each={hunkLines(id).add}>{(l) => <span class="a">{l || ' '}</span>}</For>
                      </pre>
                    </div>
                  );
                }}
              </For>
            </section>
          )}
        </For>
      </div>
    </div>
  );
};

// ======================================================================= session
const TOOL: Record<string, { past: string; now?: string; icon: Component<{ class?: string }>; noun?: string }> = {
  read_note: { past: 'Read', icon: Eye, noun: 'read a note' },
  search_notes: { past: 'Searched notes for', icon: Search, noun: 'searched notes' },
  grep: { past: 'Searched code for', icon: Search, noun: 'searched code' },
  write_file: { past: 'Edited', now: 'Edit', icon: Pencil },
  write_note: { past: 'Edited', now: 'Edit', icon: Pencil },
};
type ToolItem = Extract<Item, { t: 'tool' }>;
const isQuiet = (it: Item): it is ToolItem => it.t === 'tool' && !it.hunk && it.st === 'ok';

const ToolLine: Component<{ it: ToolItem }> = (props) => {
  const k = () => TOOL[props.it.name] ?? { past: props.it.name, icon: Pencil };
  const h = () => (props.it.hunk ? state.hunks[props.it.hunk] : undefined);
  const lines = () => (props.it.hunk ? hunkLines(props.it.hunk) : null);
  const open = () => !!state.open[props.it.id];
  const verb = () => (props.it.st === 'ask' || props.it.st === 'err' ? k().now ?? k().past : k().past);
  return (
    <div class="mk-tl" data-call={props.it.id}>
      <div class="mk-tlrow">
        <button type="button" class="mk-tltoggle" aria-expanded={open()} onClick={() => setState('open', props.it.id, !open())}>
          {k().icon({ class: 'mk-i' })}
          <span class="mk-v">{verb()}</span>
        </button>
        <Show when={props.it.path} fallback={<span class="mk-q">{props.it.arg}</span>}>
          <button type="button" class="mk-f" title={`Open ${props.it.path}`} onClick={() => openNote(props.it.path!, { fromSession: true })}>
            {basename(props.it.path!)}.md
          </button>
        </Show>
        <Show when={lines() && props.it.st === 'review' && h()?.state !== 'rejected'}>
          <span class="mk-stat">
            <Show when={lines()!.add.length}><span class="a">+{lines()!.add.length}</span></Show>
            <Show when={lines()!.del.length}><span class="d">−{lines()!.del.length}</span></Show>
          </span>
        </Show>
        <Show when={props.it.st === 'ask'}><span class="mk-tlst attn"><span class="mk-mark-need" />Waiting</span></Show>
        <Show when={props.it.st === 'err'}><span class="mk-tlst err">{props.it.out}</span></Show>
        <Show when={props.it.st === 'review' && h()?.state === 'pending'}><span class="mk-tlst attn" title="Waits for your review"><span class="mk-mark-owe" /></span></Show>
        <Show when={props.it.st === 'review' && h()?.state === 'rejected'}><span class="mk-tlst">Reverted</span></Show>
        <button type="button" class="mk-chev" aria-label="Details" aria-expanded={open()} onClick={() => setState('open', props.it.id, !open())}>
          <ChevronRight class="mk-i" />
        </button>
      </div>
      <Show when={open()}>
        <div class="mk-tlbody">
          <Show when={h() && lines()} fallback={<div class="mk-out">{props.it.out}</div>}>
            <pre class="mk-mini">
              <For each={lines()!.del}>{(l) => <span class="d">{l || ' '}</span>}</For>
              <For each={lines()!.add}>{(l) => <span class="a">{l || ' '}</span>}</For>
            </pre>
            <Show when={h()!.state === 'pending'}>
              <div class="mk-tlacts">
                <button type="button" class="mk-btn sm ghost" onClick={() => openNote(h()!.path, { fromSession: true })}>
                  <Eye class="mk-i" />Show in note
                </button>
                <span class="mk-grow" />
                <button type="button" class="mk-btn sm ghost danger" onClick={() => decide([props.it.hunk!], false)}>Reject</button>
                <button type="button" class="mk-btn sm" onClick={() => decide([props.it.hunk!], true)}><Check class="mk-i" />Accept</button>
              </div>
            </Show>
          </Show>
        </div>
      </Show>
    </div>
  );
};

/** Consecutive quiet calls fold into one counted line. */
const ToolGroup: Component<{ items: ToolItem[] }> = (props) => {
  const id = () => `grp-${props.items[0]!.id}`;
  const text = () => {
    const counts = new Map<string, number>();
    for (const it of props.items) {
      const noun = TOOL[it.name]?.noun ?? it.name;
      counts.set(noun, (counts.get(noun) ?? 0) + 1);
    }
    const parts = [...counts].map(([noun, n]) => (n > 1 ? `${noun} ×${n}` : noun));
    const joined = parts.join(', ');
    return joined.charAt(0).toUpperCase() + joined.slice(1);
  };
  return (
    <div class="mk-tl">
      <div class="mk-tlrow">
        <button type="button" class="mk-tltoggle" aria-expanded={!!state.open[id()]} onClick={() => setState('open', id(), !state.open[id()])}>
          <Sparkles class="mk-i" />
          <span class="mk-v">{text()}</span>
        </button>
        <button type="button" class="mk-chev" aria-label="Details" aria-expanded={!!state.open[id()]} onClick={() => setState('open', id(), !state.open[id()])}>
          <ChevronRight class="mk-i" />
        </button>
      </div>
      <Show when={state.open[id()]}>
        <div class="mk-tlbody"><For each={props.items}>{(it) => <ToolLine it={it} />}</For></div>
      </Show>
    </div>
  );
};

type Block = { kind: 'item'; it: Item; last: boolean } | { kind: 'group'; items: ToolItem[] };
function blocks(items: Item[]): Block[] {
  const out: Block[] = [];
  let quiet: ToolItem[] = [];
  const lastText = items.map((i) => i.t).lastIndexOf('text');
  const flush = () => {
    if (quiet.length > 1) out.push({ kind: 'group', items: quiet });
    else if (quiet.length === 1) out.push({ kind: 'item', it: quiet[0]!, last: false });
    quiet = [];
  };
  items.forEach((it, i) => {
    if (isQuiet(it)) {
      quiet.push(it);
      return;
    }
    flush();
    out.push({ kind: 'item', it, last: i === lastText });
  });
  flush();
  return out;
}

const Ring: Component<{ pct: number }> = (props) => {
  const c = 2 * Math.PI * 6;
  return (
    <svg class="mk-ring" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="6" />
      <circle cx="8" cy="8" r="6" class="v" style={{ 'stroke-dasharray': `${(props.pct / 100) * c} ${c}` }} />
    </svg>
  );
};

const ItemView: Component<{ it: Item; last: boolean }> = (props) => {
  const it = props.it;
  if (it.t === 'user')
    return (
      <div class="mk-user">
        <div class="mk-bubble" {...linkHandlers(true)} innerHTML={renderMarkdown(it.text)} />
        <div class="mk-meta-row hov">{it.time}</div>
      </div>
    );
  if (it.t === 'precog')
    return (
      <details class="mk-precog">
        <summary><Sparkles class="mk-i" />{it.notes.length} notes recalled</summary>
        <ul>
          <For each={it.notes}>
            {([p, sc]) => (
              <li>
                <button type="button" class="mk-link" onClick={() => openNote(p, { fromSession: true })}>{basename(p)}</button>
                <span class="mk-score">{sc.toFixed(2)}</span>
              </li>
            )}
          </For>
        </ul>
      </details>
    );
  if (it.t === 'thinking') return <div class="mk-thinking"><Brain class="mk-i" />Thought for {it.secs} s</div>;
  if (it.t === 'record') return <div class="mk-record">{it.text}</div>;
  if (it.t === 'tool') return <ToolLine it={it} />;
  return (
    <div class="mk-aturn">
      <div class="mk-atext" {...linkHandlers(true)} innerHTML={renderMarkdown(it.md)} />
      <Show when={it.elapsed}>
        <div class="mk-meta-row" classList={{ hov: !props.last }}>
          <span title={it.tokens ? `${it.tokens} tokens` : undefined}>{it.elapsed}</span>
        </div>
      </Show>
    </div>
  );
};

/** One session. In the right rail it shows the active session; as a centre tab it shows its own. */
export const SessionView: Component<{ sid?: string }> = (props) => {
  const sid = () => props.sid ?? state.active;
  const s = () => state.sessions[sid()]!;
  const perm = () => state.perms[sid()];
  const owed = () => pendingHunks(sid()).length;
  const expanded = () => windowStore.expandedEdge === 'right';
  let root: HTMLDivElement | undefined;
  // While the session covers the centre, a peek floats at the right edge.
  // The transcript makes room for it instead of running under it.
  const peekRoom = () => {
    if (!expanded()) return 0;
    const peekWin = windowStore.floatingWindows.find(
      (w) => !w.isMinimized && windowStore.tabGroups[w.tabGroupId]?.tabs.some((t) => t.id.startsWith('peek:')),
    );
    // Room only where the transcript keeps a readable column (about 34rem)
    // beside the peek; a narrower window lets the peek float over it.
    if (!peekWin || window.innerWidth - peekWin.width < 900) return 0;
    // Measured from this view's own right edge: the shell's gaps and the
    // peek's inset both sit between that edge and the window edge.
    const right = root?.getBoundingClientRect().right ?? window.innerWidth;
    return Math.max(0, right - peekWin.x) + 24;
  };
  const ctx = () => {
    const n = focusedNote();
    return n && !state.ctxOff[n] ? n : null;
  };
  return (
    <div class="mk-session" ref={root}>
      <div class="mk-shead">
        <span class="mk-ident" style={{ background: s().color }} />
        <span class="mk-stitle">{s().title}</span>
        <span class="mk-ctxring" title={`${s().ctx}% of the context window used`}><Ring pct={s().ctx} /></span>
        <Show when={owed()}>
          <button type="button" class="mk-iconbtn mk-reviewbtn" title={`${owed()} to review`} onClick={() => openChanges(sid())}>
            <GitCompare class="mk-i" />{owed()}
          </button>
        </Show>
        <button
          type="button"
          class="mk-iconbtn"
          title={`${expanded() ? 'Back to the documents' : 'Cover the centre'} (Shift+Esc)`}
          aria-pressed={expanded()}
          onClick={() => withTransition(() => windowActions.toggleEdgeExpanded('right'))}
        >
          <Show when={expanded()} fallback={<Maximize2 class="mk-i" />}><Minimize2 class="mk-i" /></Show>
        </button>
        <button type="button" class="mk-iconbtn" title="More"><MoreHorizontal class="mk-i" /></button>
      </div>
      <div class="mk-scroll mk-transcript" style={{ 'padding-right': peekRoom() ? `${peekRoom()}px` : undefined }}>
        <div class="mk-tinner">
          <For each={blocks(state.transcripts[sid()] ?? [])}>
            {(b) => (b.kind === 'group' ? <ToolGroup items={b.items} /> : <ItemView it={b.it} last={b.last} />)}
          </For>
        </div>
      </div>
      <div class="mk-composerwrap" style={{ 'padding-right': peekRoom() ? `${peekRoom()}px` : undefined }}>
        <div class="mk-cinner">
          <Show when={perm()}>
            {(p) => (
              <div class="mk-perm" role="alertdialog" aria-label="Permission request">
                <div class="mk-ph">Edit <code>{basename(p().path)}</code>?</div>
                <pre>
                  <For each={p().lines}>{(l) => <span class="a">{l || ' '}</span>}</For>
                </pre>
                <div class="mk-pa">
                  <button type="button" class="mk-btn sm primary" onClick={() => answerPermission(sid(), 'once')}>Allow</button>
                  <button type="button" class="mk-btn sm" onClick={() => answerPermission(sid(), 'session')}>Allow for session</button>
                  <span class="mk-grow" />
                  <button type="button" class="mk-btn sm ghost" onClick={() => answerPermission(sid(), 'deny')}>Deny</button>
                </div>
              </div>
            )}
          </Show>
          <For each={state.queue[sid()] ?? []}>
            {(text, i) => (
              <div class="mk-queued">
                <span class="mk-qlabel">Queued</span>
                <span class="mk-qtext">{text}</span>
                <button type="button" class="mk-btn sm" title="Send now, without waiting for the turn to end" onClick={() => sendQueuedNow(sid(), i())}>
                  <ArrowUp class="mk-i" />Send now
                </button>
                <button type="button" class="mk-iconbtn" aria-label="Remove" onClick={() => removeQueued(sid(), i())}><X class="mk-i" /></button>
              </div>
            )}
          </For>
          <Show when={!s().plugin} fallback={<div class="mk-record">Started by a plugin</div>}>
            <div class="mk-composer">
              <textarea
                rows={1}
                placeholder="Message"
                aria-label="Message"
                value={state.drafts[sid()] ?? ''}
                onInput={(e) => setState('drafts', sid(), e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
                    e.preventDefault();
                    send(sid());
                  }
                }}
              />
              <div class="mk-crow">
                <button type="button" class="mk-iconbtn" title="Attach a note or a kiln"><Plus class="mk-i" /></button>
                <Show when={ctx()}>
                  {(n) => (
                    <span class="mk-chip" title="The open note goes with your message">
                      <Link class="mk-i" />
                      <span class="mk-t">{basename(n())}</span>
                      <button type="button" aria-label="Do not send this note" onClick={() => setState('ctxOff', n(), true)}><X class="mk-i" /></button>
                    </span>
                  )}
                </Show>
                <span class="mk-grow" />
                <button type="button" class="mk-quietbtn" title="Permission mode">{s().mode}</button>
                <button type="button" class="mk-quietbtn" title="Model">{s().model}<ChevronDown class="mk-i" /></button>
                <button type="button" class="mk-iconbtn" title="Hold to dictate"><Mic class="mk-i" /></button>
                <button
                  type="button"
                  class="mk-send"
                  classList={{ queue: s().status === 'run' }}
                  aria-label={s().status === 'run' ? 'Queue the message' : 'Send'}
                  title={s().status === 'run' ? 'Waits for the turn to end' : 'Send'}
                  disabled={!(state.drafts[sid()] ?? '').trim()}
                  onClick={() => send(sid())}
                >
                  <ArrowUp class="mk-i" />
                </button>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
};

// ====================================================================== terminal
export const TerminalView: Component = () => (
  <div class="mk-scroll mk-term">
    <div><span class="mk-prompt">~/crucible</span> <span class="mk-cmd">cru process</span></div>
    <div class="mk-quiet">Processing kiln via daemon...</div>
    <div class="mk-quiet">Discovered: 131 indexable files · Skipped (unchanged): 131</div>
    <div><span class="mk-prompt">~/crucible</span> <span class="mk-caret" /></div>
  </div>
);
