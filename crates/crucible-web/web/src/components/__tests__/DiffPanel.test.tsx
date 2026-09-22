import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent, within } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';
import type { SentRequest } from '@/test-utils/mock-fetch';
import { getBus } from '@/lib/bus';
import type { DiffComment, DiffFileEntry, Diffset, DiffsetSource, ListedComment } from '@/lib/diffset';
import type { Proposal, ProposalState } from '@/lib/proposal-api';

const { DiffPanel } = await import('../DiffPanel');

// The panel reads the real API layer. The mock `fetch` answers the diff
// routes, so each case asserts what went on the wire.

const source: DiffsetSource = { kind: 'branch', root: '/repo', base: '', head: null };

function entry(path: string, over: Partial<DiffFileEntry> = {}): DiffFileEntry {
  return {
    root: '/repo',
    path,
    status: { kind: 'modified' },
    added: 2,
    removed: 1,
    binary: false,
    too_large: false,
    ...over,
  };
}

function diffset(files: DiffFileEntry[]): Diffset {
  return {
    id: 'branch-0123456789abcdef0123456789abcdef',
    source: { kind: 'branch', root: '/repo', base: 'master', head: null },
    files,
    unreadable_roots: [],
  };
}

function comment(id: string, over: Partial<DiffComment> = {}): DiffComment {
  return {
    id,
    diffset: 'branch-0123456789abcdef0123456789abcdef',
    anchor: { kind: 'commit', id: 'abc123' },
    author: 'human',
    body: 'a note',
    created_at: '2026-09-21T10:00:00Z',
    line_range: { start: 2, end: 3 },
    root: '/repo',
    path: 'src/a.rs',
    quoted: '2\n',
    resolved: false,
    side: 'current',
    ...over,
  };
}

let env: TestQueryEnv;

function serve(files: DiffFileEntry[], comments: ListedComment[] = []): void {
  env = createTestQueryEnv({
    'GET /api/diff': { body: diffset(files) },
    'GET /api/diff/file': { body: { base_text: 'one\ntwo\n', current_text: 'one\n2\nthree\n' } },
    'GET /api/diff/comments': { body: { diffset: 'branch-0123456789abcdef0123456789abcdef', comments } },
    'POST /api/diff/comment': {
      body: { diffset: 'branch-0123456789abcdef0123456789abcdef', comment: comment('c-new') },
    },
  });
}

beforeEach(() => {
  // A proposal pane holds the system stream open, so that `proposal_changed`
  // refetches it. jsdom has no `EventSource`.
  installFakeEventSource();
  resetGlobalRegistry();
});

afterEach(() => {
  cleanup();
  env.restore();
});

/** The section of one file. */
const section = (path: string, root = '/repo') => screen.getByTestId(`diff-file-${root}:${path}`);
/** The header button of one file, which collapses and expands it. */
const toggle = (path: string) => within(section(path)).getByTestId('diff-file-toggle');
/** The line number of one line in the editor of one file. */
async function lineNumber(path: string, line: number): Promise<HTMLElement> {
  let found: HTMLElement | null = null;
  await waitFor(() => {
    found = section(path).querySelector<HTMLElement>(`[data-testid="diff-line-${line}"]`);
    expect(found).not.toBeNull();
  });
  return found!;
}
/** Every request that went to one method and path, in order. */
async function sentTo(method: string, path: string): Promise<SentRequest[]> {
  const out: SentRequest[] = [];
  for (let i = 0; i < env.fetch.mock.calls.length; i++) {
    const sent = await env.fetch.sent(i);
    if (sent.method === method && sent.path === path) out.push(sent);
  }
  return out;
}
// The gutter draws a line number again when its state changes, so each
// event below asks for the element that the editor shows now.

/** A press on one line number. */
async function press(path: string, line: number): Promise<void> {
  fireEvent.mouseDown(await lineNumber(path, line), { button: 0, buttons: 1 });
}
/** The pointer moves over one line number, with the button down or up. */
async function over(path: string, line: number, buttons = 1): Promise<void> {
  fireEvent.mouseOver(await lineNumber(path, line), { buttons });
}
/** A release on one line number. */
async function release(path: string, line: number): Promise<void> {
  fireEvent.mouseUp(await lineNumber(path, line), { button: 0 });
}

describe('DiffPanel', () => {
  it('registers "diff" in the centre', () => {
    serve([]);
    registerPanels();
    expect(getGlobalRegistry().get('diff')?.defaultZone).toBe('center');
  });

  it('lists one section per file with its counts', async () => {
    serve([entry('src/a.rs'), entry('src/b.rs', { added: 7, removed: 0, status: { kind: 'added' } })]);
    render(() => <DiffPanel source={source} />);

    await waitFor(() => expect(section('src/a.rs')).toBeInTheDocument());
    expect(screen.getAllByTestId(/^diff-file-\/repo:src\//)).toHaveLength(2);
    expect(within(section('src/a.rs')).getByTestId('diff-file-counts').textContent).toBe('+2 −1');
    expect(within(section('src/b.rs')).getByTestId('diff-file-counts').textContent).toBe('+7 −0');
    // The header sums the files and names the base the daemon resolved.
    expect(screen.getByTestId('diff-counts').textContent).toBe('+9 −1');
    expect(screen.getByTestId('diff-source').textContent).toContain('master');
    // An empty base asks the daemon for the default branch.
    const sent = await env.fetch.sent(0);
    expect(sent.path).toBe('/api/diff');
    expect(sent.query.get('root')).toBe('/repo');
    expect(sent.query.has('base')).toBe(false);
  });

  it('a section collapses on click', async () => {
    serve([entry('src/a.rs')]);
    render(() => <DiffPanel source={source} />);

    await waitFor(() => expect(section('src/a.rs')).toBeInTheDocument());
    expect(toggle('src/a.rs').getAttribute('aria-expanded')).toBe('true');
    await waitFor(() => expect(section('src/a.rs').querySelector('.cm-editor')).not.toBeNull());

    fireEvent.click(toggle('src/a.rs'));

    expect(toggle('src/a.rs').getAttribute('aria-expanded')).toBe('false');
    expect(section('src/a.rs').querySelector('.cm-editor')).toBeNull();
  });

  it('loads the text only when a file expands', async () => {
    serve([entry('src/big.rs', { added: 300, removed: 101 })]);
    render(() => <DiffPanel source={source} />);

    await waitFor(() => expect(section('src/big.rs')).toBeInTheDocument());
    expect(env.fetch.calls('GET /api/diff/file')).toBe(0);

    fireEvent.click(toggle('src/big.rs'));

    await waitFor(() => expect(section('src/big.rs').querySelector('.cm-editor')).not.toBeNull());
    expect(env.fetch.calls('GET /api/diff/file')).toBe(1);
    const [sent] = await sentTo('GET', '/api/diff/file');
    expect(sent.query.get('path')).toBe('src/big.rs');
  });

  it('a large file starts collapsed', async () => {
    serve([entry('src/a.rs', { added: 200, removed: 200 }), entry('src/big.rs', { added: 300, removed: 101 })]);
    render(() => <DiffPanel source={source} />);

    await waitFor(() => expect(section('src/big.rs')).toBeInTheDocument());
    // 400 changed lines is the limit. One more starts collapsed.
    expect(toggle('src/a.rs').getAttribute('aria-expanded')).toBe('true');
    expect(toggle('src/big.rs').getAttribute('aria-expanded')).toBe('false');
  });

  describe('the focus target', () => {
    let scrolled: string[];
    beforeEach(() => {
      scrolled = [];
      // jsdom does not scroll. The spy records the section that asked.
      Element.prototype.scrollIntoView = function (this: Element) {
        scrolled.push(this.getAttribute('data-testid') ?? '');
      };
    });
    afterEach(() => {
      delete (Element.prototype as Partial<Element>).scrollIntoView;
    });

    const files = () => [
      entry('src/a.rs'),
      entry('src/big.rs', { added: 300, removed: 101 }),
      entry('src/big.rs', { root: '/other', added: 300, removed: 101 }),
    ];

    it('expands and scrolls to the file of a root and a path', async () => {
      serve(files());
      render(() => <DiffPanel source={source} focus={{ root: '/repo', path: 'src/big.rs', seq: 1 }} />);

      await waitFor(() => expect(toggle('src/big.rs').getAttribute('aria-expanded')).toBe('true'));
      await waitFor(() => expect(scrolled).toEqual(['diff-file-/repo:src/big.rs']));
      // The same path in another root keeps the size rule.
      expect(within(section('src/big.rs', '/other')).getByTestId('diff-file-toggle').getAttribute('aria-expanded')).toBe(
        'false',
      );
    });

    // A tool call names its file by the absolute path.
    it('finds the file of an absolute path', async () => {
      serve(files());
      render(() => <DiffPanel source={source} focus={{ path: '/other/src/big.rs', seq: 1 }} />);

      await waitFor(() => expect(scrolled).toEqual(['diff-file-/other:src/big.rs']));
      expect(within(section('src/big.rs', '/other')).getByTestId('diff-file-toggle').getAttribute('aria-expanded')).toBe(
        'true',
      );
      expect(toggle('src/big.rs').getAttribute('aria-expanded')).toBe('false');
    });

    it('a new sequence number focuses the file again', async () => {
      serve(files());
      const [focus, setFocus] = createSignal({ root: '/repo', path: 'src/big.rs', seq: 1 });
      render(() => <DiffPanel source={source} focus={focus()} />);

      await waitFor(() => expect(scrolled).toHaveLength(1));
      // The user collapses the file, and then asks for it again.
      fireEvent.click(toggle('src/big.rs'));
      expect(toggle('src/big.rs').getAttribute('aria-expanded')).toBe('false');
      setFocus({ root: '/repo', path: 'src/big.rs', seq: 2 });

      await waitFor(() => expect(toggle('src/big.rs').getAttribute('aria-expanded')).toBe('true'));
      await waitFor(() => expect(scrolled).toHaveLength(2));
    });
  });

  it('a binary file shows a line and no editor', async () => {
    serve([entry('img.png', { binary: true, added: 0, removed: 0 })]);
    render(() => <DiffPanel source={source} />);

    await waitFor(() => expect(section('img.png')).toBeInTheDocument());
    expect(within(section('img.png')).getByText('Binary file. There is no text to show.')).toBeInTheDocument();
    expect(section('img.png').querySelector('.cm-editor')).toBeNull();
    expect(env.fetch.calls('GET /api/diff/file')).toBe(0);
  });

  it('two roots with the same path do not collide', async () => {
    const record: DiffsetSource = { kind: 'session_record', session: 'chat-1' };
    env = createTestQueryEnv({
      'GET /api/diff': {
        body: {
          id: 'session-chat-1',
          source: record,
          files: [entry('a.md', { root: '/one' }), entry('a.md', { root: '/two' })],
          unreadable_roots: [],
        },
      },
      'GET /api/diff/file': (request: Request) => {
        const root = new URL(request.url).searchParams.get('root');
        return { base_text: `${root}\n`, current_text: `${root}\nadded\n` };
      },
      'GET /api/diff/comments': { body: { diffset: 'session-chat-1', comments: [] } },
    });
    render(() => <DiffPanel source={record} />);

    await waitFor(() => expect(section('a.md', '/two')).toBeInTheDocument());
    expect(section('a.md', '/one')).not.toBe(section('a.md', '/two'));
    // Each section asks for the text of its own root, and shows it.
    await waitFor(() => expect(section('a.md', '/one').textContent).toContain('/one'));
    await waitFor(() => expect(section('a.md', '/two').textContent).toContain('/two'));
    expect(section('a.md', '/one').textContent).not.toContain('/two');
    expect(env.fetch.calls('GET /api/diff/file')).toBe(2);
  });

  it('a session record names each root that the daemon cannot read', async () => {
    const record: DiffsetSource = { kind: 'session_record', session: 'chat-1' };
    env = createTestQueryEnv({
      'GET /api/diff': {
        body: {
          id: 'session-chat-1',
          source: record,
          files: [entry('a.md', { root: '/one' })],
          unreadable_roots: [
            { root: '/gone', reason: 'tracked root no longer exists' },
            { root: '/old', reason: 'session base snapshot abc is no longer stored' },
          ],
        } satisfies Diffset,
      },
      'GET /api/diff/file': { body: { base_text: 'a\n', current_text: 'b\n' } },
      'GET /api/diff/comments': { body: { diffset: 'session-chat-1', comments: [] } },
    });
    render(() => <DiffPanel source={record} />);

    const banner = await screen.findByTestId('diff-unreadable-roots');
    const rows = within(banner).getAllByTestId('diff-unreadable-root');
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining('/gone'),
      expect.stringContaining('/old'),
    ]);
    expect(rows[0].textContent).toContain('tracked root no longer exists');
    expect(rows[1].textContent).toContain('session base snapshot abc is no longer stored');
    // The readable root still lists its files below the banner.
    expect(section('a.md', '/one')).toBeInTheDocument();
  });

  it('a diffset with no unreadable root shows no banner', async () => {
    serve([entry('src/a.rs')]);
    render(() => <DiffPanel source={source} />);
    await waitFor(() => expect(section('src/a.rs')).toBeInTheDocument());
    expect(screen.queryByTestId('diff-unreadable-roots')).toBeNull();
  });

  it('hovering a line number shows the add button', async () => {
    serve([entry('src/a.rs')]);
    render(() => <DiffPanel source={source} />);

    await lineNumber('src/a.rs', 2);
    expect(within(section('src/a.rs')).queryByTestId('diff-comment-add')).toBeNull();

    await over('src/a.rs', 2, 0);

    await waitFor(async () =>
      expect(within(await lineNumber('src/a.rs', 2)).getByTestId('diff-comment-add')).toBeInTheDocument(),
    );
    // Only the line under the pointer has the button.
    expect(within(section('src/a.rs')).getAllByTestId('diff-comment-add')).toHaveLength(1);
  });

  it('a drag over line numbers selects a range', async () => {
    serve([entry('src/a.rs')]);
    render(() => <DiffPanel source={source} />);

    await press('src/a.rs', 1);
    await over('src/a.rs', 2);
    await over('src/a.rs', 3);
    await release('src/a.rs', 3);

    const box = await within(section('src/a.rs')).findByTestId('diff-comment-box');
    expect(box.textContent).toContain('Lines 1-3');
    for (const line of [1, 2, 3]) {
      expect((await lineNumber('src/a.rs', line)).getAttribute('aria-selected')).toBe('true');
    }
    // The box sits under the last line of the range.
    const lines = [...section('src/a.rs').querySelectorAll('.cm-content > *')];
    const boxAt = lines.findIndex((el) => el.contains(box));
    expect(lines[boxAt - 1]?.textContent).toBe('three');
  });

  it('ctrl enter posts the comment', async () => {
    serve([entry('src/a.rs')]);
    render(() => <DiffPanel source={source} />);

    await press('src/a.rs', 2);
    await release('src/a.rs', 2);
    const input = await within(section('src/a.rs')).findByTestId('diff-comment-input');
    fireEvent.input(input, { target: { value: 'this needs a test' } });
    fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });

    await waitFor(() => expect(env.fetch.calls('POST /api/diff/comment')).toBe(1));
    const sent = (await sentTo('POST', '/api/diff/comment'))[0];
    expect(sent.body).toEqual({
      source,
      path: 'src/a.rs',
      line_start: 2,
      line_end: 3,
      side: 'current',
      body: 'this needs a test',
    });
    // The box closes after the daemon stores the comment.
    await waitFor(() => expect(within(section('src/a.rs')).queryByTestId('diff-comment-box')).toBeNull());
  });

  it('send to chat inserts a reference', async () => {
    serve([entry('src/a.rs')]);
    const inserted = vi.fn();
    getBus().on('insertIntoComposer', inserted);
    render(() => <DiffPanel source={source} />);

    await press('src/a.rs', 1);
    await over('src/a.rs', 2);
    await release('src/a.rs', 2);
    const input = await within(section('src/a.rs')).findByTestId('diff-comment-input');
    fireEvent.input(input, { target: { value: 'why this?' } });
    fireEvent.click(within(section('src/a.rs')).getByTestId('diff-comment-send'));

    expect(inserted).toHaveBeenCalledWith({ text: '@src/a.rs:1-2 why this?' });
    // Send to chat does not store a comment.
    expect(env.fetch.calls('POST /api/diff/comment')).toBe(0);
    await waitFor(() => expect(within(section('src/a.rs')).queryByTestId('diff-comment-box')).toBeNull());
  });

  it('copy comments writes the quickfix form', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    serve(
      [entry('src/a.rs')],
      [
        { comment: comment('c1', { line_range: { start: 626, end: 629 }, body: 'this deny path needs a test' }), outdated: false },
        { comment: comment('c2', { line_range: { start: 4, end: 5 }, body: 'first\nsecond' }), outdated: false },
        { comment: comment('c3', { body: 'done', resolved: true }), outdated: false },
      ],
    );
    render(() => <DiffPanel source={source} />);

    const copy = screen.getByTestId('diff-copy-comments');
    await waitFor(() => expect(copy.hasAttribute('disabled')).toBe(false));
    fireEvent.click(copy);

    // A resolved comment is not open, so the list leaves it out.
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        'src/a.rs:626: [626-628] this deny path needs a test\nsrc/a.rs:4: first\n  second\n',
      ),
    );
  });

  it('an outdated comment shows at the end of its file', async () => {
    serve(
      [entry('src/a.rs')],
      [
        { comment: comment('c1', { body: 'still here' }), outdated: false },
        { comment: comment('c2', { body: 'text is gone', line_range: { start: 9, end: 10 } }), outdated: true },
      ],
    );
    render(() => <DiffPanel source={source} />);

    await lineNumber('src/a.rs', 1);
    const outdated = await within(section('src/a.rs')).findByTestId('diff-comment-outdated');
    expect(outdated.textContent).toContain('text is gone');
    // It is after the editor, and not inside it.
    const editor = within(section('src/a.rs')).getByTestId('diff-file-editor');
    expect(editor.contains(outdated)).toBe(false);
    expect(editor.compareDocumentPosition(outdated) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The comment that is not outdated shows in the editor, under its line.
    await waitFor(() => expect(within(editor).getByTestId('diff-comment').textContent).toContain('still here'));
  });

  describe('a proposal', () => {
    const ID = '7a1c2f3e-0000-4000-8000-000000000001';
    const proposalSource: DiffsetSource = { kind: 'proposal', id: ID };
    const ACCEPT = `/api/proposals/${ID}/accept`;

    function proposal(state: ProposalState, over: Partial<Proposal> = {}): Proposal {
      return {
        id: ID,
        author: { kind: 'plugin', name: 'reflection' },
        title: 'Change 2 notes',
        created_at: '2026-09-21T10:00:00Z',
        state,
        writes: [
          { root: '/kiln', path: 'a.md', base: { kind: 'absent' }, new_text: 'a\n' },
          { root: '/kiln', path: 'b.md', base: { kind: 'absent' }, new_text: 'b\n' },
        ],
        ...over,
      } as Proposal;
    }

    /** The routes of one proposal in `state`. Each decision answers `reply`. */
    function serveProposal(state: ProposalState, reply: Proposal = proposal({ kind: 'accepted' })): void {
      env = createTestQueryEnv({
        'GET /api/diff': {
          body: {
            id: `proposal-${ID}`,
            source: proposalSource,
            files: [entry('a.md', { root: '/kiln', status: { kind: 'added' } }), entry('b.md', { root: '/kiln' })],
          },
        },
        'GET /api/diff/file': { body: { base_text: null, current_text: 'a\n' } },
        'GET /api/diff/comments': { body: { diffset: `proposal-${ID}`, comments: [] } },
        [`GET /api/proposals/${ID}`]: { body: proposal(state) },
        [`POST ${ACCEPT}`]: { body: reply },
        [`POST /api/proposals/${ID}/reject`]: { body: proposal({ kind: 'rejected' }) },
        [`POST /api/proposals/${ID}/resolve`]: { body: proposal({ kind: 'accepted' }) },
      });
    }

    it('proposal actions show only for a proposal', async () => {
      serve([entry('src/a.rs')]);
      render(() => <DiffPanel source={source} />);
      await waitFor(() => expect(section('src/a.rs')).toBeInTheDocument());
      expect(screen.queryByTestId('proposal-bar')).toBeNull();
      expect(screen.queryByTestId('proposal-accept-all')).toBeNull();
      expect(screen.queryByTestId('proposal-accept-file')).toBeNull();
      cleanup();
      env.restore();

      serveProposal({ kind: 'open' });
      render(() => <DiffPanel source={proposalSource} />);
      await screen.findByTestId('proposal-accept-all');
      expect(screen.getByTestId('proposal-reject-all')).toBeInTheDocument();
      expect(screen.getByTestId('proposal-title').textContent).toBe('Change 2 notes');
      await waitFor(() =>
        expect(within(section('b.md', '/kiln')).getByTestId('proposal-accept-file')).toBeInTheDocument(),
      );
      expect(within(section('a.md', '/kiln')).getByTestId('proposal-reject-file')).toBeInTheDocument();
      // The files of a proposal load by the proposal id and the root of the file.
      const [list] = await sentTo('GET', '/api/diff');
      expect(list.query.get('proposal')).toBe(ID);
    });

    it('a decided proposal shows no actions', async () => {
      serveProposal({ kind: 'accepted' });
      render(() => <DiffPanel source={proposalSource} />);
      await waitFor(() => expect(screen.getByTestId('proposal-state').textContent).toBe('Accepted'));
      await waitFor(() => expect(section('a.md', '/kiln')).toBeInTheDocument());
      expect(screen.queryByTestId('proposal-accept-all')).toBeNull();
      expect(screen.queryByTestId('proposal-reject-all')).toBeNull();
      expect(screen.queryByTestId('proposal-accept-file')).toBeNull();
    });

    it('accept all calls the accept route', async () => {
      serveProposal({ kind: 'open' });
      render(() => <DiffPanel source={proposalSource} />);
      fireEvent.click(await screen.findByTestId('proposal-accept-all'));

      await waitFor(() => expect(env.fetch.calls(`POST ${ACCEPT}`)).toBe(1));
      const [sent] = await sentTo('POST', ACCEPT);
      expect(sent.body, 'no paths: every file').toEqual({});
      // The pane reads the proposal again, to show the new state.
      await waitFor(() => expect(env.fetch.calls(`GET /api/proposals/${ID}`)).toBe(2));
    });

    it('accept on a file sends its path', async () => {
      serveProposal({ kind: 'open' }, proposal({ kind: 'accepted' }, { id: 'other' }));
      render(() => <DiffPanel source={proposalSource} />);
      const accept = await waitFor(() => within(section('b.md', '/kiln')).getByTestId('proposal-accept-file'));
      fireEvent.click(accept);

      await waitFor(() => expect(env.fetch.calls(`POST ${ACCEPT}`)).toBe(1));
      const [sent] = await sentTo('POST', ACCEPT);
      expect(sent.body).toEqual({ paths: ['b.md'] });
    });

    it('a conflicted proposal shows its regions', async () => {
      serveProposal({
        kind: 'conflicted',
        files: [
          {
            root: '/kiln',
            path: 'a.md',
            disk_text: 'one\nDISK\n',
            merged_text: 'one\nMINE\n',
            regions: [{ start_line: 2, end_line: 3, base: 'two\n', ours: 'MINE\n', theirs: 'DISK\n' }],
          },
        ],
      });
      render(() => <DiffPanel source={proposalSource} />);

      const conflict = await screen.findByTestId('proposal-conflict-/kiln:a.md');
      await waitFor(() => expect(within(conflict).getByTestId('conflict-region-0')).toBeInTheDocument());
      // The conflict view takes the place of the files and of Accept all.
      expect(screen.queryByTestId('diff-file-/kiln:a.md')).toBeNull();
      expect(screen.queryByTestId('proposal-accept-all')).toBeNull();
      const save = within(conflict).getByTestId('conflict-save');
      expect(save.textContent).toBe('Accept resolution');

      fireEvent.click(within(conflict).getByTestId('keep-theirs-0'));
      await waitFor(() => expect(save).not.toBeDisabled());
      fireEvent.click(save);

      await waitFor(() => expect(env.fetch.calls(`POST /api/proposals/${ID}/resolve`)).toBe(1));
      const [sent] = await sentTo('POST', `/api/proposals/${ID}/resolve`);
      expect(sent.body).toEqual({ path: 'a.md', text: 'one\nDISK\n' });
    });
  });
});
