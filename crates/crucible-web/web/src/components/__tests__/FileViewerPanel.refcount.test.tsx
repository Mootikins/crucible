import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { createSignal, Show } from 'solid-js';
import { createTestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

// Regression for the open-file refcount leak: FileViewerPanel's open effect
// must depend ONLY on props.filePath. openFile() starts with a reactive store
// read (openFilesStore.find), so without untrack the effect subscribes to the
// open-files array and re-runs whenever the store mutates — the file's own
// async load push, or any other panel (e.g. a hover popover) opening a file —
// re-incrementing the refcount so the buffer is never evicted (ghost tab that
// resurrects stale edits). These tests use the REAL EditorContext + real panel
// effect, stubbing only the heavy editor child and side-effecting stores.

// The moved helper (`lib/paths.ts`), stubbed where it lives now.
vi.mock('@/lib/paths', () => ({
  rawFileUrl: (p: string) => `/api/file/raw?path=${encodeURIComponent(p)}`,
}));

// No `vi.mock('@/lib/api')`: the buffer's read — content AND the hash an
// offline write anchors on — the daemon config and the note listing all
// arrive over the ROUTES in `beforeEach`.
vi.mock('../editor/EditorWithPreview', () => ({
  EditorWithPreview: () => <div data-testid="editor-stub" />,
}));
vi.mock('@/lib/file-actions', () => ({ findTabByFilePath: vi.fn(() => null) }));
vi.mock('@/lib/note-actions', () => ({
  openNoteInEditor: vi.fn(async () => {}),
  kilnForPath: vi.fn(() => undefined),
}));
vi.mock('@/stores/windowStore', () => ({
  windowActions: { updateTab: vi.fn() },
  windowStore: { tabGroups: {}, layout: { id: 'pane-1', type: 'pane', tabGroupId: null } },
}));
vi.mock('@/stores/statusBarStore', () => ({ statusBarStore: { kilnPath: () => null } }));

const { EditorProvider, useEditor } = await import('@/contexts/EditorContext');
const { default: FileViewerPanel } = await import('../FileViewerPanel');

const flush = async () => {
  // Eviction is deferred a microtask; give the queue a couple of turns.
  await Promise.resolve();
  await Promise.resolve();
};

// The panel asks which kiln owns the open file. Nothing here is in one, and
// the empty roster now arrives over the fetch instead of from a module stub.
let kilnEnv: ReturnType<typeof createTestQueryEnv>;

describe('FileViewerPanel — open refcount does not leak', () => {
  beforeEach(() => {
    resetKilnsForTests();
    kilnEnv = createTestQueryEnv({
      'GET /api/kilns': () => ({ kilns: [] }),
      // The same route answers the plain-text read and the hashed one the
      // offline layer asks for.
      'GET /api/kiln/file': () => ({ content: 'content\n', content_hash: 'h' }),
      'PUT /api/kiln/file': () => ({}),
      'GET /api/config': () => ({ kiln_path: '/kiln', config_root: '/etc/crucible' }),
      'GET /api/notes': () => ({ notes: [] }),
    });
  });

  afterEach(() => {
    kilnEnv.restore();
    resetKilnsForTests();
  });

  it('evicts the buffer when a single mounted panel unmounts (self-load must not double-count)', async () => {
    const A = '/k/a.md';
    let editor!: ReturnType<typeof useEditor>;
    const [showA, setShowA] = createSignal(true);
    const Probe = () => {
      editor = useEditor();
      return null;
    };

    render(() => (
      <EditorProvider>
        <Probe />
        <Show when={showA()}>
          <FileViewerPanel filePath={A} />
        </Show>
      </EditorProvider>
    ));

    await waitFor(() => expect(editor.openFiles().some((f) => f.path === A)).toBe(true));

    // Unmount the only holder → onCleanup force-closes A. A refcount inflated by
    // the load-push re-run would leave A as a ghost; a correct count evicts it.
    setShowA(false);
    await flush();

    expect(editor.openFiles().some((f) => f.path === A)).toBe(false);
  });

  it('a mounted panel is unaffected by another file opening (hover-popover churn)', async () => {
    const A = '/k/a.md';
    const B = '/k/b.md';
    let editor!: ReturnType<typeof useEditor>;
    const [showA, setShowA] = createSignal(true);
    const Probe = () => {
      editor = useEditor();
      return null;
    };

    render(() => (
      <EditorProvider>
        <Probe />
        <Show when={showA()}>
          <FileViewerPanel filePath={A} />
        </Show>
      </EditorProvider>
    ));

    await waitFor(() => expect(editor.openFiles().some((f) => f.path === A)).toBe(true));

    // Simulate a hover-preview popover loading B into the shared store.
    await editor.openFile(B);
    await flush();

    // Closing A's tab must still fully release it despite the store churn.
    setShowA(false);
    await flush();

    expect(editor.openFiles().some((f) => f.path === A)).toBe(false);
    expect(editor.openFiles().some((f) => f.path === B)).toBe(true);
  });
});
