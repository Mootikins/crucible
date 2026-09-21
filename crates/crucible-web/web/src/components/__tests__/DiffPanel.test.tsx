import { describe, it, expect, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent, within } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';
import type { DiffFileEntry, Diffset, DiffsetSource } from '@/lib/diffset';

const { DiffPanel } = await import('../DiffPanel');

// The panel reads the real API layer. The mock `fetch` answers the two diff
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
  };
}

let env: TestQueryEnv;

function serve(files: DiffFileEntry[]): void {
  env = createTestQueryEnv({
    'GET /api/diff': { body: diffset(files) },
    'GET /api/diff/file': { body: { base_text: 'one\ntwo\n', current_text: 'one\n2\nthree\n' } },
  });
}

beforeEach(() => {
  resetGlobalRegistry();
});

afterEach(() => {
  cleanup();
  env.restore();
});

/** The section of one file. */
const section = (path: string) => screen.getByTestId(`diff-file-${path}`);
/** The header button of one file, which collapses and expands it. */
const toggle = (path: string) => within(section(path)).getByTestId('diff-file-toggle');

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
    expect(screen.getAllByTestId(/^diff-file-src\//)).toHaveLength(2);
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
    const sent = await env.fetch.sent(1);
    expect(sent.path).toBe('/api/diff/file');
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

  it('a binary file shows a line and no editor', async () => {
    serve([entry('img.png', { binary: true, added: 0, removed: 0 })]);
    render(() => <DiffPanel source={source} />);

    await waitFor(() => expect(section('img.png')).toBeInTheDocument());
    expect(within(section('img.png')).getByText('Binary file. There is no text to show.')).toBeInTheDocument();
    expect(section('img.png').querySelector('.cm-editor')).toBeNull();
    expect(env.fetch.calls('GET /api/diff/file')).toBe(0);
  });
});
