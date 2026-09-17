import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

/**
 * A failed save used to strand a dirty buffer behind a red line with no way
 * out — and nothing on screen said that pressing save again WAS the way out.
 * The context now hands the banner the actual failed call.
 */

// No `vi.mock('@/lib/api')`. The editor writes through the offline layer onto
// the daemon's own routes; the two below answer, scripted by the spies, so a
// case that fails a call fails it ON THE WIRE and the retry re-issues it there.
const getFileContent = vi.fn(async (_path: string) => '');
const saveFileContent = vi.fn(async (_path: string, _content: string) => {});

// The moved helper (`lib/paths.ts`), stubbed where it lives now.
vi.mock('@/lib/paths', () => ({
  rawFileUrl: (p: string) => `/api/file/raw?path=${encodeURIComponent(p)}`,
}));

const KILN = '/home/user/kiln';
const PATH = `${KILN}/notes/dirty.md`;

const { createTestQueryEnv } = await import('@/test-utils/query');
const { resetKilnsForTests } = await import('@/lib/query/kilns');
const { installFakeEventSource } = await import('@/test-utils/sse');

// The routes the roster resolves against, and the editor reads and writes on.
let kilnEnv: ReturnType<typeof createTestQueryEnv>;

beforeEach(() => {
  installFakeEventSource();
  resetKilnsForTests();
  kilnEnv = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: [{ path: KILN, name: 'kiln' }] }),
    // The read: the bytes with the hash they were read at.
    'GET /api/kiln/file': async (request) => ({
      content: await getFileContent(new URL(request.url).searchParams.get('path')!),
      content_hash: 'base-hash',
    }),
    // The write: the body that reached the daemon goes through the spy, so
    // the assertions below read one place.
    'PUT /api/kiln/file': async (request) => {
      const body = (await request.clone().json()) as { path: string; content: string };
      await saveFileContent(body.path, body.content);
      return { ok: true, content_hash: 'written' };
    },
    'GET /api/notes': () => ({ notes: [] }),
    'GET /api/config': () => ({ kiln_path: KILN, config_root: '/etc/crucible' }),
  });
});

afterEach(() => {
  kilnEnv.restore();
  resetKilnsForTests();
});

const { EditorProvider, useEditor } = await import('../EditorContext');

function mountEditor(): ReturnType<typeof useEditor> {
  let captured: ReturnType<typeof useEditor> | undefined;
  const Probe = () => {
    captured = useEditor();
    return <div data-testid="probe">{captured.openFiles().length}</div>;
  };
  render(() => (
    <EditorProvider>
      <Probe />
    </EditorProvider>
  ));
  return captured!;
}

describe('EditorContext — a failed operation keeps its own retry', () => {
  beforeEach(() => {
    getFileContent.mockClear();
    saveFileContent.mockClear();
  });

  it('offers nothing to retry while nothing has failed', () => {
    const editor = mountEditor();
    expect(editor.retryFailedOperation()).toBeNull();
  });

  it('a failed save re-issues the SAVE, and the buffer keeps the user’s bytes', async () => {
    const editor = mountEditor();
    getFileContent.mockResolvedValueOnce('on disk\n');
    await editor.openFile(PATH);
    editor.updateFileContent(PATH, 'edited but unwritten\n');

    saveFileContent.mockRejectedValueOnce(new Error('disk is full'));
    await editor.saveFile(PATH);

    await waitFor(() => expect(editor.error()).toBe('disk is full'));
    // Still dirty, still holding the edit — this is the state the banner has
    // to rescue, and a reload would destroy it.
    const dirty = editor.openFiles().find((f) => f.path === PATH);
    expect(dirty?.dirty).toBe(true);
    expect(dirty?.content).toBe('edited but unwritten\n');

    const retry = editor.retryFailedOperation();
    expect(retry).not.toBeNull();

    saveFileContent.mockResolvedValueOnce(undefined);
    await retry!();

    // The retry wrote the buffer, and did NOT re-read the file.
    expect(saveFileContent).toHaveBeenLastCalledWith(PATH, 'edited but unwritten\n');
    expect(getFileContent).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(editor.error()).toBeNull();
      expect(editor.openFiles().find((f) => f.path === PATH)?.dirty).toBe(false);
    });
    expect(editor.retryFailedOperation()).toBeNull();
  });

  it('a failed open re-issues the OPEN, not the save', async () => {
    // The panel knows the ACTIVE file; the failure may belong to a background
    // open of a different one, so the retry has to come from the call itself.
    const editor = mountEditor();
    getFileContent.mockRejectedValueOnce(new Error('no such file'));
    await editor.openFile(PATH);

    await waitFor(() => expect(editor.error()).toBe('no such file'));
    getFileContent.mockResolvedValueOnce('recovered\n');
    await editor.retryFailedOperation()!();

    expect(saveFileContent).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(editor.openFiles().find((f) => f.path === PATH)?.content).toBe('recovered\n'),
    );
  });
});
