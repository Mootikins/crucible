import type { Page } from '@playwright/test';
import type { CommentRef, DiffComment, DiffsetSource, NewDiffComment } from '@/lib/diffset';
import { disableAnimations } from './geometry';
import {
  MOCK_SESSION,
  MOCK_SESSION_DETAIL,
  MOCK_PROVIDERS,
  MOCK_KILNS,
  MOCK_CONFIG,
  MOCK_PROJECT,
  MOCK_PUBLICATIONS,
  MOCK_PLUGIN_TARGETS,
  MOCK_DIFF_FILES,
  MOCK_DIFF_TEXTS,
} from './fixtures';
import { mockEventsRoute } from './mock-sse';

export interface MockOverrides {
  sessions?: object[];
  providers?: object;
  config?: object;
  publications?: object;
  /** Per-command answers for `POST /api/rpc/plugin.run_command`, by command name. */
  pluginTargets?: Record<string, { targets: object[] }>;
  kilns?: object;
  projects?: object[];
  sessionHistory?: object;
  /** `{ status: [StatusDisplayItem, …] }` — the per-session status route. */
  sessionStatus?: object;
  sseEvents?: Array<{ type: string; data: object }>;
  sessionCreate?: object;
  /** The file entries of `diff.get`, without `root`. The mock adds the asked root. */
  diffFiles?: object[];
  /** The texts of `diff.file`, by path. */
  diffTexts?: Record<string, { base_text: string | null; current_text: string | null }>;
  chatMessage?: object | number;
  /**
   * The proposals of `proposal.list` (`POST /api/rpc/proposal.list`), as the
   * daemon sends them. The proposal diffset of `diff.get` (`source.kind:
   * 'proposal'`) lists their writes.
   */
  proposals?: Array<{
    id: string;
    writes: Array<{ root: string; path: string; new_text: string }>;
  }>;
}

/** One body of `POST /api/chat/send`, as the browser sent it. */
interface SentMessage {
  session_id: string;
  content: string;
  /** The references of the attached comments. The text of a comment is not here. */
  comments?: CommentRef[];
}

/** What the mock recorded, for the assertions of a spec. */
export interface MockApi {
  /** The comments that `diff.comment` (`POST /api/rpc/diff.comment`) stored, oldest first. */
  comments: DiffComment[];
  /** The messages that `POST /api/chat/send` took, oldest first. */
  sent: SentMessage[];
}

/** The id of the diffset of one source, as the daemon derives it. */
function diffsetId(source: DiffsetSource | undefined): string {
  if (source?.kind === 'session_record') return `session-${source.session}`;
  if (source?.kind === 'proposal') return `proposal-${source.id}`;
  return 'branch-00000000000000000000000000000000';
}

/** What the daemon compared with, by the kind of the source. */
function anchorOf(source: DiffsetSource | undefined): DiffComment['anchor'] {
  if (source?.kind === 'session_record') return { kind: 'snapshot', id: `snap-${source.session}` };
  if (source?.kind === 'proposal') return { kind: 'proposal', id: source.id };
  return { kind: 'commit', id: 'mock-merge-base' };
}

export async function setupBasicMocks(page: Page, overrides: MockOverrides = {}): Promise<MockApi> {
  const recorded: MockApi = { comments: [], sent: [] };
  // The id of each stored comment counts up on its own. A deleted comment
  // must not give its id back, as the daemon's ids do not repeat.
  let comments = 0;
  // Animations off before anything renders — see disableAnimations().
  await disableAnimations(page);

  // `project.list` reaches the browser through `POST /api/rpc/{method}` now
  // (Simplification Plan step 19 item 3); `GET /api/project/list` is gone.
  await page.route('**/api/rpc/project.list', (route) =>
    route.fulfill({ json: overrides.projects ?? [MOCK_PROJECT] }),
  );

  // `session.list`, `session.get`, `session.status`,
  // `session.list_notifications`, `session.history`, `session.list_modes`
  // and `providers.list` only forwarded one RPC row each and are gone
  // ([[Simplification Plan#Step 19]] item 9): the browser calls
  // `rpc(method, params)` through `POST /api/rpc/{method}` now, so each
  // mock matches the one method path instead of a per-session URL.
  await page.route('**/api/rpc/session.list', (route) =>
    route.fulfill({
      json: {
        sessions: overrides.sessions ?? [MOCK_SESSION],
        total: (overrides.sessions ?? [MOCK_SESSION]).length,
      },
    }),
  );

  await page.route('**/api/rpc/session.get', (route) =>
    // session.get returns the nested-agent detail shape, not the list shape.
    route.fulfill({ json: MOCK_SESSION_DETAIL }),
  );

  await page.route('**/api/rpc/session.status', (route) =>
    route.fulfill({ json: overrides.sessionStatus ?? { status: [] } }),
  );
  // The notifications a chat reads once when it attaches to a session.
  await page.route('**/api/rpc/session.list_notifications', (route) =>
    route.fulfill({ json: { notifications: [] } }),
  );

  await page.route('**/api/rpc/session.history', (route) =>
    route.fulfill({
      json: overrides.sessionHistory ?? {
        session_id: MOCK_SESSION.session_id,
        history: [],
        total_events: 0,
        transcript: { as_of_seq: 0, items: [] },
      },
    }),
  );

  // The one shared connection (Simplification Plan step 19): a chat
  // session's own topic answers `sseEvents`, and the `system` topic (the
  // filesystem watcher, surfaces, publications and proposals) answers
  // nothing, matching the two mocks this used to be.
  await mockEventsRoute(page, overrides.sseEvents ?? []);

  await page.route('**/api/interactions/pending', (route) =>
    route.fulfill({ json: { pending: [] } }),
  );

  await page.route('**/api/rpc/session.list_modes', (route) =>
    route.fulfill({ json: { current_mode_id: 'ask', modes: [] } }),
  );

  // `fs.list_dir` reaches the browser through `POST /api/rpc/{method}` now
  // (Simplification Plan step 19 item 3); `GET /api/fs/list` is gone.
  await page.route('**/api/rpc/fs.list_dir', (route) =>
    route.fulfill({ json: { entries: [], truncated: false } }),
  );

  await page.route('**/api/recents', (route) => route.fulfill({ json: { recents: [] } }));

  await page.route('**/api/rpc/providers.list', (route) =>
    route.fulfill({ json: overrides.providers ?? MOCK_PROVIDERS }),
  );

  await page.route('**/api/config', (route) =>
    route.fulfill({ json: overrides.config ?? MOCK_CONFIG }),
  );

  // `plugin.publications` and `plugin.run_command` reach the browser
  // through `POST /api/rpc/{method}` now (Simplification Plan step 19
  // item 4), with the caller header `rpc(..., { caller })` sends.
  await page.route('**/api/rpc/plugin.publications', (route) =>
    route.fulfill({ json: overrides.publications ?? MOCK_PUBLICATIONS }),
  );

  // Target enumeration. A command rather than more published data because the
  // workspace axis is per-project: the branch list belongs to a repository.
  await page.route('**/api/rpc/plugin.run_command', (route) => {
    const { name } = JSON.parse(route.request().postData() ?? '{}');
    const answers = overrides.pluginTargets ?? MOCK_PLUGIN_TARGETS;
    route.fulfill({ json: answers[name] ?? { targets: [] } });
  });

  // `kiln.list` reaches the browser through `POST /api/rpc/{method}` now
  // (Simplification Plan step 19 item 3); `GET /api/kilns` is gone, and the
  // reply is the array directly, not `{ kilns }`.
  await page.route('**/api/rpc/kiln.list', (route) =>
    route.fulfill({ json: overrides.kilns ?? MOCK_KILNS }),
  );

  // The diffsets. A branch reply names the default branch, as the daemon
  // does for an empty base. Both rows reach the daemon through
  // `POST /api/rpc/{method}` now (Simplification Plan step 19), with the
  // tagged `DiffsetSource` in the body rather than a flat query string.
  await page.route('**/api/rpc/diff.get', (route) => {
    const { source } = route.request().postDataJSON() as { source: DiffsetSource };
    if (source.kind === 'session_record') {
      // A session record. The mock session changed no file.
      return route.fulfill({
        json: { id: `session-${source.session}`, source, files: [], unreadable_roots: [] },
      });
    }
    if (source.kind === 'proposal') {
      // A proposal diffset lists the writes of the proposal. Each write
      // adds its lines to a note, which is enough for the counts.
      const proposal = (overrides.proposals ?? []).find((p) => p.id === source.id);
      return route.fulfill({
        json: {
          id: `proposal-${source.id}`,
          source,
          files: (proposal?.writes ?? []).map((write) => ({
            root: write.root,
            path: write.path,
            status: { kind: 'modified' },
            added: write.new_text.split('\n').filter(Boolean).length,
            removed: 0,
            binary: false,
            too_large: false,
          })),
          unreadable_roots: [],
        },
      });
    }
    route.fulfill({
      json: {
        id: 'branch-00000000000000000000000000000000',
        source,
        files: (overrides.diffFiles ?? MOCK_DIFF_FILES).map((file) => ({
          root: source.root,
          ...file,
        })),
        unreadable_roots: [],
      },
    });
  });

  // The comment store of the daemon, in memory. A POST keeps the comment, so
  // the listing that follows shows it under its lines, as the daemon does.
  // The reply carries the id, which the composer chip then references.
  await page.route('**/api/rpc/diff.comment', (route) => {
    const body = route.request().postDataJSON() as NewDiffComment;
    comments += 1;
    const comment: DiffComment = {
      id: `c-${comments}`,
      diffset: diffsetId(body.source),
      root: body.root ?? (body.source.kind === 'branch' ? body.source.root : ''),
      path: body.path,
      anchor: anchorOf(body.source),
      side: body.side,
      line_range: { start: body.line_start, end: body.line_end ?? body.line_start + 1 },
      quoted: '',
      body: body.body,
      author: body.author ?? 'human',
      resolved: false,
      created_at: '2026-09-22T10:00:00Z',
    };
    recorded.comments.push(comment);
    return route.fulfill({ json: { diffset: comment.diffset, comment } });
  });

  // The `×` of a composer chip deletes the comment, so the pane loses it
  // too. Delete is not resolve: the comment leaves the store.
  await page.route('**/api/rpc/diff.delete_comment', (route) => {
    const body = route.request().postDataJSON() as { source: DiffsetSource; comment_id: string };
    const at = recorded.comments.findIndex((c) => c.id === body.comment_id);
    if (at < 0) {
      return route.fulfill({
        status: 422,
        json: { error: { code: 422, message: `unknown comment ${body.comment_id}` } },
      });
    }
    const [gone] = recorded.comments.splice(at, 1);
    return route.fulfill({
      json: { diffset: gone.diffset, comment_id: gone.id, deleted: true },
    });
  });

  // The comments of a diffset: the ones this page stored. None is outdated,
  // because the mock texts do not change under a comment.
  await page.route('**/api/rpc/diff.comments', (route) => {
    const { source } = route.request().postDataJSON() as { source: DiffsetSource };
    const diffset = diffsetId(source);
    const comments = recorded.comments
      .filter((c) => c.diffset === diffset && !c.resolved)
      .map((comment) => ({ comment, outdated: false }));
    return route.fulfill({ json: { diffset, comments } });
  });

  await page.route('**/api/rpc/diff.file', (route) => {
    const { path } = route.request().postDataJSON() as { path: string };
    const text = (overrides.diffTexts ?? MOCK_DIFF_TEXTS)[path];
    if (!text) return route.fulfill({ status: 404, json: { error: `no file ${path}` } });
    return route.fulfill({ json: text });
  });

  // The proposals in the Inbox, and one proposal by id. The `system` topic
  // of the shared connection carries `proposal_changed`; the mock sends none
  // (registered once, above, for every topic). Both rows reach the daemon
  // through `POST /api/rpc/{method}` now (Simplification Plan step 19), with
  // the id in the body rather than the path.
  await page.route('**/api/rpc/proposal.list', (route) =>
    route.fulfill({ json: overrides.proposals ?? [] }),
  );
  await page.route('**/api/rpc/proposal.get', (route) => {
    const { id } = route.request().postDataJSON() as { id: string };
    const proposal = (overrides.proposals ?? []).find((p) => p.id === id);
    if (!proposal) return route.fulfill({ status: 422, json: { error: `no proposal ${id}` } });
    return route.fulfill({ json: proposal });
  });

  // Draft-session panel loads (lazy session creation surface). Both rows
  // reach the daemon through `POST /api/rpc/{method}` now (Simplification
  // Plan step 19), not their own REST route.
  await page.route('**/api/rpc/agents.list_profiles', (route) =>
    route.fulfill({ json: { profiles: [] } }),
  );
  await page.route('**/api/rpc/models.list', (route) =>
    route.fulfill({ json: { models: ['llama3.2'] } }),
  );

  await page.route('**/api/layout', (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ status: 404, body: '' });
    } else {
      route.fulfill({ status: 200, body: '' });
    }
  });

  // The terminal is a WebSocket, which `page.route` never sees. Unmocked, it
  // reaches whatever listens on the API port through the dev server's proxy:
  // a daemon that refuses the upgrade makes the browser log a console error,
  // and a spec that asserts a clean console fails on a socket it never asked
  // for. This mock PTY accepts the socket and says nothing. A spec that needs
  // a prompt registers its own handler after this one; the last-added route
  // wins.
  await page.routeWebSocket('**/api/terminal/ws', () => {});

  await page.route('**/api/session', async (route) => {
    if (route.request().method() === 'POST') {
      route.fulfill({ json: overrides.sessionCreate ?? MOCK_SESSION });
    } else {
      route.continue();
    }
  });

  // `session.send_message`, `session.generate_title`, `session.set_title`
  // and `session.list_models` only forwarded one RPC row each and are gone
  // too; the send route keeps each body, so a spec can read what the
  // composer carried: the content, and the references of the attached
  // comments.
  await page.route('**/api/rpc/session.send_message', async (route) => {
    recorded.sent.push(route.request().postDataJSON() as SentMessage);
    const override = overrides.chatMessage;
    if (typeof override === 'number') {
      route.fulfill({ status: override, body: 'Error' });
    } else {
      route.fulfill({ json: override ?? { message_id: 'msg-001' } });
    }
  });

  await page.route('**/api/rpc/session.generate_title', (route) =>
    route.fulfill({ json: { title: 'Auto-generated Title' } }),
  );

  await page.route('**/api/rpc/session.set_title', (route) =>
    route.fulfill({ status: 200, body: '{}' }),
  );

  await page.route('**/api/rpc/session.list_models', (route) =>
    route.fulfill({ json: { models: ['llama3.2', 'mistral'] } }),
  );

  return recorded;
}
