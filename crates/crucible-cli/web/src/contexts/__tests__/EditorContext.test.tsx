import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// get_note_by_name returns metadata only (no content), so the editor must load
// file bytes via GET /api/kiln/file (getFileContent). These mocks let us assert
// which endpoint openFile actually hits.
const getFileContent = vi.fn(async (_path: string) => '');
const saveNote = vi.fn(async () => {});
const getNote = vi.fn(async () => ({ name: '', path: '', content: '', title: null, tags: [], updated_at: '' }));

vi.mock('@/lib/api', () => ({
  getFileContent: (p: string) => getFileContent(p),
  saveNote: (...args: unknown[]) => saveNote(...(args as [])),
  getNote: () => getNote(),
}));

const KILN = '/home/user/kiln';
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => ({
      path: '/home/user/project',
      name: 'p',
      kilns: [{ path: KILN, name: 'k' }],
      last_accessed: '',
    }),
  }),
}));

const { EditorProvider, useEditor } = await import('../EditorContext');

function withEditor(fn: (editor: ReturnType<typeof useEditor>) => void) {
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
  fn(captured!);
  return captured!;
}

describe('EditorContext — content load path (bug 8)', () => {
  beforeEach(() => {
    getFileContent.mockClear();
    saveNote.mockClear();
    getNote.mockClear();
  });

  it('openFile loads via getFileContent (GET /api/kiln/file), not getNote', async () => {
    const path = `${KILN}/notes/from-tui.md`;
    getFileContent.mockResolvedValueOnce('terminal was here\n');

    let editor: ReturnType<typeof useEditor>;
    editor = withEditor(() => {});
    await editor!.openFile(path);

    expect(getFileContent).toHaveBeenCalledWith(path);
    expect(getNote).not.toHaveBeenCalled();

    await waitFor(() => {
      const f = editor!.openFiles().find((x) => x.path === path);
      expect(f?.content).toBe('terminal was here\n');
      expect(f?.dirty).toBe(false);
    });
  });

  it('edit marks dirty; saveFile PUTs content via saveNote and clears dirty', async () => {
    const path = `${KILN}/notes/from-tui.md`;
    getFileContent.mockResolvedValueOnce('start\n');

    const editor = withEditor(() => {});
    await editor.openFile(path);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));

    editor.updateFileContent(path, 'start\nbrowser was here\n');
    expect(editor.openFiles()[0].dirty).toBe(true);

    await editor.saveFile(path);

    // Save goes through PUT /api/notes/:name (saveNote), which writes to disk.
    expect(saveNote).toHaveBeenCalledWith('notes/from-tui', KILN, 'start\nbrowser was here\n');
    await waitFor(() => expect(editor.openFiles()[0].dirty).toBe(false));
  });
});
