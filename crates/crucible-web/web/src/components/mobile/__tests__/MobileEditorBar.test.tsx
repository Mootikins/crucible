import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

const save = vi.hoisted(() => vi.fn());
const files = vi.hoisted(() => ({ list: [] as { path: string; dirty: boolean }[] }));
vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({ openFiles: () => files.list, saveFile: save }),
}));

import { MobileEditorBar } from '@/components/mobile/MobileEditorBar';
import { compactEditorMode, setCompactEditorMode } from '@/stores/editorModeStore';

beforeEach(() => {
  save.mockClear();
  files.list = [{ path: '/kiln/a.md', dirty: false }];
  setCompactEditorMode('live');
});

describe('MobileEditorBar', () => {
  it('switches the open note between reading and writing', () => {
    render(() => <MobileEditorBar filePath="/kiln/a.md" />);
    fireEvent.click(screen.getByRole('button', { name: 'Read' }));
    expect(compactEditorMode()).toBe('reading');
    fireEvent.click(screen.getByRole('button', { name: 'Write' }));
    expect(compactEditorMode()).toBe('live');
  });

  it('says which mode the note is in', () => {
    render(() => <MobileEditorBar filePath="/kiln/a.md" />);
    expect(screen.getByRole('button', { name: 'Write' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Read' }).getAttribute('aria-pressed')).toBe('false');
  });

  // The desktop draws the unsaved dot in its corner bar, which a phone has not.
  it('shows an unsaved mark and saves on demand, only while a buffer is dirty', () => {
    render(() => <MobileEditorBar filePath="/kiln/a.md" />);
    expect(screen.queryByRole('button', { name: 'Save' })).toBeNull();

    files.list = [{ path: '/kiln/a.md', dirty: true }];
    render(() => <MobileEditorBar filePath="/kiln/a.md" />);
    fireEvent.click(screen.getAllByRole('button', { name: 'Save' })[0]);
    expect(save).toHaveBeenCalledWith('/kiln/a.md');
  });

  it('gives every control a touch-sized target', () => {
    files.list = [{ path: '/kiln/a.md', dirty: true }];
    render(() => <MobileEditorBar filePath="/kiln/a.md" />);
    for (const name of ['Read', 'Write', 'Save']) {
      expect(screen.getByRole('button', { name }).className).toMatch(/\bh-11\b/);
    }
  });
});
