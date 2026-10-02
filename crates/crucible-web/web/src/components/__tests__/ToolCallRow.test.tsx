import { render, fireEvent, screen } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';
import { ToolCallRow } from '../ToolCallRow';
import { openNoteInEditor } from '@/lib/note-actions';
import { openFileInEditor } from '@/lib/file-actions';
vi.mock('@/lib/note-actions', () => ({ openNoteInEditor: vi.fn(), kilnForElement: (el: Element) => el.closest('[data-kiln]')?.getAttribute('data-kiln') }));
vi.mock('@/lib/file-actions', () => ({ openFileInEditor: vi.fn(), fileOpenOptionsForEvent: () => ({ where: 'tab' }) }));
describe('tool file navigation', () => {
  it.each(['Search & Discovery.md', '/project/src/main.rs'])('opens %s in its owning context', path => {
    vi.clearAllMocks();
    render(() => <div data-kiln="/kilns/docs"><ToolCallRow expanded={false} onToggle={() => {}} toolCall={{ id: 'read', name: 'read_note', args: '', status: 'complete', display: { kind: 'file_read', tool: 'read_note', paths: [path] } }} /></div>);
    fireEvent.click(screen.getByRole('button', { name: `Open ${path}` }));
    if (path.startsWith('/')) {
      expect(openFileInEditor).toHaveBeenCalledWith(path, undefined, { where: 'tab' });
      expect(openNoteInEditor).not.toHaveBeenCalled();
    } else {
      expect(openNoteInEditor).toHaveBeenCalledWith(path, '/kilns/docs', { where: 'tab' });
      expect(openFileInEditor).not.toHaveBeenCalled();
    }
  });
});
