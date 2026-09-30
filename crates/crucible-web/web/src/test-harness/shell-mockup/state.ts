/**
 * Mockup state: sessions, transcripts, review hunks, the permission request
 * and the message queue. Synthetic; nothing here talks to a daemon.
 */
import { createStore, produce } from 'solid-js/store';
import { createSignal } from 'solid-js';
import { NOTE_TEXT } from './data';
import type { RootKey } from './components/files/FileTree';
import type { HunkState } from './components/review/types';
import type { PermissionChoice } from './components/session/PermissionCard';
import type { TranscriptItem } from './components/session/types';
import type { SessionMarkStatus } from './components/sessions/types';

export interface Session {
  title: string;
  group: string;
  roots: RootKey[];
  /** A categorical canvas slot, reused as the session's identity colour. */
  color: string;
  status: SessionMarkStatus;
  time: string;
  model: string;
  mode: 'Ask' | 'Auto' | 'Plan';
  ctx: number;
  plugin?: boolean;
}

/**
 * A pending edit that waits for permission: the call, the file, and the
 * lines it inserts above the line `before`. The real request carries
 * `diffs` (a `FileDiff` with the old and the new content) instead.
 */
export interface PermissionRequest {
  call: string;
  tool: string;
  path: string;
  before: string;
  lines: string[];
}

/** A transcript item. The components own the shape; this store fills it. */
type Item = TranscriptItem;

export interface Hunk {
  session: string;
  path: string;
  call?: string;
  state: HunkState;
  external?: boolean;
  /** Lines of a hunk whose note carries no text in the mockup. */
  del?: string[];
  add?: string[];
}

/** The label of the group of sessions that have no project. */
export const NO_PROJECT = 'Chats';

const canvas = (slot: string) => `var(--cru-color-canvas-${slot})`;

const [state, setState] = createStore({
  sessions: {
    s1: { title: 'Tighten the Precognition note', group: NO_PROJECT, roots: ['folder', 'docs'], color: canvas('cyan'), status: 'need', time: '2 m', model: 'glm-5.2', mode: 'Ask', ctx: 18 },
    s4: { title: 'Draft Windows setup troubleshooting', group: NO_PROJECT, roots: ['folder', 'docs'], color: canvas('orange'), status: 'idle', time: 'Yesterday', model: 'glm-5.2', mode: 'Ask', ctx: 31 },
    s2: { title: 'Explain proposal acceptance', group: 'crucible', roots: ['crucible', 'docs'], color: canvas('green'), status: 'run', time: 'now', model: 'glm-5.2', mode: 'Auto', ctx: 9 },
    s5: { title: 'Check kiln chip copy against the glossary', group: 'crucible', roots: ['crucible', 'docs'], color: canvas('yellow'), status: 'idle', time: '3 d', model: 'glm-5.2', mode: 'Plan', ctx: 12 },
    s3: { title: 'Nightly reflection pass', group: 'Reflections', roots: ['docs'], color: canvas('purple'), status: 'owe', time: '6 h', model: 'reflection', mode: 'Auto', ctx: 22, plugin: true },
  } as Record<string, Session>,
  active: 's1',
  notes: { ...NOTE_TEXT } as Record<string, string>,
  hunks: {
    h1: { session: 's1', path: 'Help/Concepts/Precognition', call: 'c3', state: 'accepted' },
    h2: { session: 's1', path: 'Help/Concepts/Precognition', call: 'c6', state: 'absent' },
    x1: { session: 's1', path: 'Guides/Getting Started', external: true, state: 'pending', del: ['Crucible is a knowledge-grounded agent runtime.'], add: ['Crucible is a knowledge-grounded agent runtime — agents that draw from a knowledge graph make better decisions.'] },
    h3: { session: 's3', path: 'Help/Concepts/Kilns', call: 'r1', state: 'pending' },
    h4: { session: 's3', path: 'Help/Tags', call: 'r2', state: 'pending', del: [], add: ['Tags in frontmatter give precognition more signal about what a note covers. Prefer a few exact tags to many loose ones.'] },
    h5: { session: 's3', path: 'Organization Styles/Zettelkasten', call: 'r3', state: 'pending', del: [], add: ['In a kiln, a wikilink does the work of a slip number: it resolves by name, and a move keeps the link.'] },
    h6: { session: 's3', path: 'Help/Concepts/Session Compaction', call: 'r4', state: 'pending', del: ['The knobs and the RPC surface exist; the compaction itself does not.'], add: ['The knobs and the RPC surface exist; the compaction itself does not. A session that reaches its budget keeps every turn.'] },
  } as Record<string, Hunk>,
  /** One pending permission per session, keyed by session id. */
  perms: {
    s1: { call: 'c6', tool: 'write_file', path: 'Help/Concepts/Precognition.md', before: '### Customizing with Lua', lines: ['### When to turn it off', '', 'Turn Precognition off when a conversation is not about your notes, for example a quick shell question. The injected notes then cost context and add nothing. Run `:set noprecognition` before the first message, because the injection happens only once.'] },
  } as Record<string, PermissionRequest>,
  transcripts: {
    s1: [
      { t: 'user', text: 'The opening of [[Precognition]] leans on a metaphor. Replace it with what a new user needs to know first.', time: '7:38 PM' },
      { t: 'precog', notes: [['Help/Concepts/Precognition', 0.91], ['Help/Concepts/Semantic Search', 0.84], ['Help/Concepts/Trust and Classification', 0.77]] },
      { t: 'thinking', secs: 6 },
      { t: 'tool', id: 'c1', name: 'read_note', path: 'Help/Concepts/Precognition', st: 'ok', out: '142 lines' },
      { t: 'tool', id: 'c2', name: 'search_notes', arg: '“first user message”', st: 'ok', out: '2 hits: Help/Concepts/Precognition, Help/Core/Sessions' },
      { t: 'tool', id: 'c3', name: 'write_file', path: 'Help/Concepts/Precognition', hunk: 'h1', st: 'review' },
      { t: 'text', md: 'I replaced the metaphor with two facts: it runs once, on the first message, and it is on by default.', elapsed: '8.9 s', tokens: '2,418' },
      { t: 'user', text: 'Now add a short "When to turn it off" part under Configuration.', time: '7:41 PM' },
      { t: 'text', md: 'I will add it after **Checking Current Settings**.' },
      { t: 'tool', id: 'c6', name: 'write_file', path: 'Help/Concepts/Precognition', hunk: 'h2', st: 'ask' },
    ],
    s2: [
      { t: 'user', text: 'What happens when I reject a proposal? Does it undo the edit?', time: '7:44 PM' },
      { t: 'precog', notes: [['Help/Concepts/Review Ledger', 0.93], ['Help/Concepts/Note Sync', 0.71]] },
      { t: 'tool', id: 'd1', name: 'read_note', path: 'Help/Concepts/Review Ledger', st: 'ok', out: '96 lines' },
      { t: 'tool', id: 'd2', name: 'grep', arg: 'proposal_reject', st: 'ok', out: '6 hits in 3 files under crates/crucible-daemon/src' },
      { t: 'text', md: 'A proposal holds suggested text separately from the note. **Reject** records the decision without changing the note; **Accept** writes the proposed text after checking its base.' },
    ],
    s3: [
      { t: 'record', text: 'Reflection on “Draft Windows setup troubleshooting”' },
      { t: 'tool', id: 'r1', name: 'write_note', path: 'Help/Concepts/Kilns', hunk: 'h3', st: 'review' },
      { t: 'tool', id: 'r2', name: 'write_note', path: 'Help/Tags', hunk: 'h4', st: 'review' },
      { t: 'tool', id: 'r3', name: 'write_note', path: 'Organization Styles/Zettelkasten', hunk: 'h5', st: 'review' },
      { t: 'tool', id: 'r4', name: 'write_note', path: 'Help/Concepts/Session Compaction', hunk: 'h6', st: 'review' },
      { t: 'text', md: 'Four edits proposed from the finished session.', elapsed: '41 s', tokens: '6,904' },
    ],
    s4: [
      { t: 'user', text: 'Draft a troubleshooting part for [[Windows Setup]]: the daemon socket, PATH, and long paths.', time: 'Yesterday' },
      { t: 'text', md: 'Added three short fixes to **Windows Setup**.', elapsed: '12.4 s', tokens: '3,112' },
    ],
    s5: [
      { t: 'user', text: 'Does the kiln chip copy match the glossary in [[CONTEXT]]?', time: 'Sep 25' },
      { t: 'text', md: 'Yes, except one tooltip that says **vault**. The glossary says **kiln**.', elapsed: '6.1 s', tokens: '1,540' },
    ],
  } as Record<string, Item[]>,
  /** Tool lines the user opened, by call id or group id. */
  open: {} as Record<string, boolean>,
  drafts: {} as Record<string, string>,
  /** Messages that wait for the running turn to end, per session. */
  queue: {} as Record<string, string[]>,
  /** The note a composer carries as context, unless the user removed it. */
  ctxOff: {} as Record<string, boolean>,
  theme: 'dark' as 'dark' | 'light',
  /** Which kind opens in the centre. The ribbon's swap button sets it; it
      moves nothing that is already open. */
  spawn: 'docs' as 'docs' | 'sessions',
  /** Each document tab's history, as in a web browser: the paths it showed,
      and the one it shows now. A tab without an entry has not moved yet. */
  history: {} as Record<string, { stack: string[]; at: number }>,
});

export { state, setState };

/** The notes each session used, for the dot on a tree row. */
export const TOUCHED: Record<string, string[]> = {
  s1: ['Help/Concepts/Precognition', 'Help/Concepts/Semantic Search'],
  s2: ['Help/Concepts/Review Ledger'],
  s3: ['Help/Concepts/Kilns', 'Help/Tags', 'Organization Styles/Zettelkasten', 'Help/Concepts/Session Compaction'],
};

/** The note that has focus in the centre, for the composer's context chip. */
export const [focusedNote, setFocusedNote] = createSignal<string | null>('Help/Concepts/Precognition');

const H2_BLOCK =
  ':::hunk h2\n+### When to turn it off\n+\n+Turn Precognition off when a conversation is not about your notes, for example a quick shell question. The injected notes then cost context and add nothing. Run `:set noprecognition` before the first message, because the injection happens only once.\n:::\n\n';

/** The hunks that still wait for a decision, for one session or for all. */
export const pendingHunks = (session?: string) =>
  Object.entries(state.hunks)
    .filter(([, h]) => h.state === 'pending' && !h.external && (!session || h.session === session))
    .map(([id]) => id);

/** The added and removed lines of a hunk, read from its note's `:::hunk` block. */
export function hunkLines(id: string): { del: string[]; add: string[] } {
  const h = state.hunks[id];
  if (!h) return { del: [], add: [] };
  if (h.del || h.add) return { del: h.del ?? [], add: h.add ?? [] };
  const m = (state.notes[h.path] ?? '').match(new RegExp(`:::hunk ${id}\\n([\\s\\S]*?)\\n:::`));
  if (!m) return { del: [], add: [] };
  const lines = m[1].split('\n');
  return {
    del: lines.filter((l) => l.startsWith('-')).map((l) => l.slice(1)),
    add: lines.filter((l) => l.startsWith('+')).map((l) => l.slice(1)),
  };
}

function refreshStatus(sid: string) {
  setState(
    'sessions',
    sid,
    produce((s) => {
      if (state.perms[sid]) s.status = 'need';
      else if (s.status !== 'run') s.status = pendingHunks(sid).length ? 'owe' : 'idle';
    }),
  );
}

/** The line of record that each wider grant leaves in the transcript. */
const GRANT_RECORD: Partial<Record<PermissionChoice, string>> = {
  session: 'Edits allowed for this session',
  project: 'Edits allowed for this project',
  user: 'Edits always allowed',
};

export function answerPermission(sid: string, choice: PermissionChoice) {
  const p = state.perms[sid];
  if (!p) return;
  setState('perms', produce((perms) => delete perms[sid]));
  setState(
    'transcripts',
    sid,
    produce((items) => {
      const card = items.find((i) => i.t === 'tool' && i.id === p.call);
      if (card && card.t === 'tool') {
        card.st = choice === 'deny' ? 'err' : 'review';
        if (choice === 'deny') card.out = 'Denied';
      }
      if (choice === 'deny') items.push({ t: 'text', md: 'OK. I did not change the note.', elapsed: '0.4 s' });
      else {
        items.push({ t: 'text', md: 'Added **When to turn it off** under Configuration.', elapsed: '5.2 s', tokens: '1,207' });
        const record = GRANT_RECORD[choice];
        if (record) items.push({ t: 'record', text: record });
      }
    }),
  );
  if (choice !== 'deny') {
    setState('notes', 'Help/Concepts/Precognition', (src) => src.replace('### Customizing with Lua', `${H2_BLOCK}### Customizing with Lua`));
    setState('hunks', 'h2', 'state', 'accepted');
  }
  refreshStatus(sid);
}

/**
 * Send, or queue behind a running turn. The mockup's running session is s2;
 * a queued message waits there until the user sends it now.
 */
export function send(sid: string) {
  const text = (state.drafts[sid] ?? '').trim();
  if (!text) return;
  setState('drafts', sid, '');
  if (state.sessions[sid]?.status === 'run') {
    setState('queue', sid, (q) => [...(q ?? []), text]);
    return;
  }
  deliver(sid, text);
}

function deliver(sid: string, text: string) {
  setState('transcripts', sid, (items) => [...items, { t: 'user', text, time: 'now' }]);
  setTimeout(() => {
    setState('transcripts', sid, (items) => [
      ...items,
      { t: 'text', md: '(Mockup: no model is connected.)', elapsed: '0.1 s' },
    ]);
  }, 400);
}

/** The override: send a queued message now, ahead of the running turn's end. */
export function sendQueuedNow(sid: string, index: number) {
  const text = state.queue[sid]?.[index];
  if (text === undefined) return;
  setState('queue', sid, (q) => q.filter((_, i) => i !== index));
  deliver(sid, text);
}

export function removeQueued(sid: string, index: number) {
  setState('queue', sid, (q) => q.filter((_, i) => i !== index));
}
