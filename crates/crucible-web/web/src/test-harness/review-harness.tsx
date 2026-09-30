/** Dev-only preview of the real in-session review component, with local RPC fixtures. */
import { render } from 'solid-js/web';
import { DiffPanel } from '@/components/DiffPanel';
import type { DiffComment, Diffset, ListedComment, NewDiffComment } from '@/lib/diffset';
import { diffsetKey } from '@/lib/diffset';
import { analyzeDiff } from '@/lib/diff-stats';
import { theme, applyTheme } from '@/lib/theme';
import { SERVER_BASE, SERVER_CURRENT } from './review-fixture';
import '@/index.css';

const source = { kind: 'session_record', session: 'review-preview' } as const;
const root = '/preview/server';
const path = 'src/server.rs';
const stats = analyzeDiff(SERVER_BASE, SERVER_CURRENT);
const diffset: Diffset = {
  id: diffsetKey(source),
  source,
  unreadable_roots: [],
  files: [
    {
      root,
      path,
      status: { kind: 'modified' },
      added: stats.additions,
      removed: stats.deletions,
      binary: false,
      too_large: false,
    },
  ],
};
let comments: ListedComment[] = [];

// This page never sends an RPC to the daemon. Rendering, hunk folding, and
// comment interaction belong to the production components; only replies are fixtures.
const nativeFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = async (input, init) => {
  const request = new Request(input, init);
  const url = new URL(request.url);
  if (!url.pathname.startsWith('/api/')) return nativeFetch(request);
  const method = url.pathname.split('/').pop();
  let reply: unknown;
  switch (method) {
    case 'session.get':
      reply = { title: 'Server cleanup' };
      break;
    case 'diff.get':
      reply = diffset;
      break;
    case 'diff.file':
      reply = { base_text: SERVER_BASE, current_text: SERVER_CURRENT };
      break;
    case 'diff.comments':
      reply = { comments };
      break;
    case 'diff.comment': {
      const body: NewDiffComment = await request.json();
      const text = body.side === 'base' ? SERVER_BASE : SERVER_CURRENT;
      const end = body.line_end ?? body.line_start + 1;
      const comment: DiffComment = {
        id: crypto.randomUUID(),
        diffset: diffset.id,
        root,
        path,
        side: body.side,
        body: body.body,
        author: 'human',
        created_at: new Date().toISOString(),
        resolved: false,
        anchor: { kind: 'snapshot', id: 'preview-base' },
        line_range: { start: body.line_start, end },
        quoted: text
          .split('\n')
          .slice(body.line_start - 1, end - 1)
          .join('\n'),
      };
      comments.push({ comment, outdated: false });
      reply = { comment };
      break;
    }
    case 'diff.resolve_comment': {
      const body = await request.json();
      comments = comments.filter((c) => c.comment.id !== body.comment_id);
      reply = { diffset: diffset.id, comment_id: body.comment_id, resolved: true };
      break;
    }
    default:
      return Response.json({ error: 'Unavailable in the local preview' }, { status: 400 });
  }
  return Response.json(reply);
};

render(
  () => (
    <main class="h-screen flex flex-col bg-shell-bg text-shell-ink">
      <header class="flex items-center justify-between px-5 py-3">
        <div>
          <h1 class="text-sm font-semibold">In-session code review</h1>
          <p class="text-xs text-muted">
            Local preview · one Rust file, two edit hunks · select line numbers to comment
          </p>
        </div>
        <button
          class="rounded bg-control px-3 py-1 text-xs focus-ring"
          onClick={() => applyTheme(theme() === 'dark' ? 'light' : 'dark')}
        >
          Toggle theme
        </button>
      </header>
      <div class="flex-1 min-h-0 mx-auto w-full max-w-5xl px-4 pb-4">
        <DiffPanel source={source} session={source.session} />
      </div>
    </main>
  ),
  document.getElementById('root')!,
);
