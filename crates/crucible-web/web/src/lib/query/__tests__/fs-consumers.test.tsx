import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { TestQueryEnv } from '@/test-utils/query';

/**
 * The two readers of the filesystem stream, on screen together.
 *
 * `FilesPanel` watches the tree and `EditorContext` watches the open buffers.
 * They were two `EventSource`s on one URL, so the daemon sent every kiln write
 * twice and each side backed off on its own schedule. Here they share the root
 * of `lib/query/sse.ts`, and this spec holds that: one source, and each
 * consumer still folds the frame its own way.
 *
 * It renders both rather than calling `fsEvents()` twice, because the fault it
 * exists to find is a consumer that goes around the root, and a direct call
 * cannot see that.
 */

const KILN = '/home/user/kiln';

const getFileContent = vi.fn(async (_path: string) => 'on disk\n');

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listNotes: async () => [],
  listDir: async () => ({ entries: [] }),
  getFileWithHash: async (path: string) => ({
    content: await getFileContent(path),
    content_hash: 'base-hash',
  }),
  getFileContent: (path: string) => getFileContent(path),
  getConfig: async () => ({ kiln_path: KILN, config_root: '/etc/crucible' }),
  getNote: async () => ({ name: '', path: '', content: '', title: null, tags: [], updated_at: '' }),
}));

// The tree roots on the session's kiln, so the panel has something to browse.
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => null,
    projects: () => [],
    refreshProjects: async () => {},
  }),
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({ session_id: 's-1', kilns: ['kiln'], workspace: null }),
    applySessionScope: () => {},
  }),
}));

const { FilesPanel } = await import('@/components/FilesPanel');
const { EditorProvider, useEditor } = await import('@/contexts/EditorContext');
const { createTestQueryEnv } = await import('@/test-utils/query');
const { resetKilnsForTests } = await import('@/lib/query/kilns');
const { FakeEventSource, installFakeEventSource } = await import('@/test-utils/sse');
const { setOfflineStore } = await import('@/lib/offline/sync');
const { memoryStore } = await import('@/lib/offline/store');

let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  // The editor asks the outbox whether the buffer has writing the daemon has
  // not taken yet. Without a store that question fails towards the banner, and
  // a clean buffer would never re-read.
  setOfflineStore(memoryStore());
  resetKilnsForTests();
  localStorage.clear();
  env = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: [{ path: KILN, name: 'kiln' }] }),
  });
});

afterEach(() => {
  env.restore();
  resetKilnsForTests();
  vi.clearAllMocks();
});

/** Renders both readers, and answers the editor so a test can open a file. */
function renderBoth() {
  let editor: ReturnType<typeof useEditor> | undefined;
  const Probe = () => {
    editor = useEditor();
    return <span data-testid="probe" />;
  };
  render(() => (
    <EditorProvider>
      <Probe />
      <FilesPanel />
    </EditorProvider>
  ));
  return editor!;
}

describe('the shared fs stream', () => {
  it('opens one EventSource for the panel and the editor together', async () => {
    const editor = renderBoth();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    await editor.openFile(`${KILN}/notes/a.md`);
    await waitFor(() => expect(editor.openFiles()).toHaveLength(1));

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.instances[0]!.url).toBe('/api/fs/events');
  });

  // Each consumer keeps its own handler on the one source. The editor's fold
  // is what a test can read from outside, so it is the one asserted.
  it('gives the editor a disk change off the source the panel opened', async () => {
    const editor = renderBoth();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const path = `${KILN}/notes/a.md`;
    await editor.openFile(path);
    await waitFor(() => expect(editor.openFiles()).toHaveLength(1));
    getFileContent.mockClear();
    getFileContent.mockResolvedValueOnce('another writer was here\n');

    FakeEventSource.instances[0]!.emit('fs_changed', {
      type: 'changed',
      path,
      kind: 'modified',
    });

    await waitFor(() =>
      expect(editor.openFiles()[0]!.content).toBe('another writer was here\n'),
    );
    expect(FakeEventSource.instances).toHaveLength(1);
  });
});
