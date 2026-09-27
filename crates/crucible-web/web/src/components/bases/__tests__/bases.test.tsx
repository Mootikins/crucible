import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { BaseView } from '../BaseView';
import { renderMarkdownDocAsync } from '@/lib/markdown';
import { baseJson, baseText } from '@/lib/query/bases';

const mocks = vi.hoisted(() => ({ query: vi.fn(), set: vi.fn(), create: vi.fn(), open: vi.fn(), note: vi.fn() }));
vi.mock('@/lib/query/kilns', () => ({ useKilns: () => ({ data: [{ name: 'Work', path: '/kiln' }] }) }));
vi.mock('@/lib/query/bases', async importOriginal => ({ ...await importOriginal<object>(), useBase: (...args: unknown[]) => mocks.query(...args), setBaseProperty: (...args: unknown[]) => mocks.set(...args), newBaseEntry: (...args: unknown[]) => mocks.create(...args) }));
vi.mock('@/lib/note-actions', () => ({ openNoteInEditor: (...args: unknown[]) => mocks.note(...args) }));
vi.mock('@/lib/file-actions', () => ({ openFileInEditor: (...args: unknown[]) => mocks.open(...args) }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const row = { path: 'a.md', ancestor_hash: 'h1', values: { 'file.name': { type: 'string', value: 'a.md' }, 'note.done': { type: 'boolean', value: false } } };
const answer = () => ({ root: '/kiln', view: 'Table', view_type: 'table', columns: [{ property: 'file.name', display_name: 'Name' }, { property: 'note.done', display_name: 'Done' }], rows: [row], groups: [], summaries: {}, group_property: null, views: [{ name: 'Table', type: 'table' }, { name: 'Cards', type: 'cards' }] });

describe('Bases', () => {
  it('WS-250: renders a typed query, opens its entry, and switches named views', async () => {
    mocks.query.mockReturnValue({ data: answer() });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    expect(screen.getByText('false')).toBeTruthy();
    fireEvent.click(screen.getByText('a.md'));
    expect(mocks.open).toHaveBeenCalledWith('/kiln/a.md');
    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'Cards' } });
    const request = mocks.query.mock.calls[0][0] as () => unknown;
    expect(request()).toEqual({ kiln: 'Work', source: { path: '/kiln/Tasks.base' }, view: 'Cards', this: undefined });
  });
  it('opens a link-valued first column in the owning kiln without opening the row', () => {
    mocks.query.mockReturnValue({ data: { ...answer(), rows: [{ ...row, values: { ...row.values, 'file.name': { type: 'link', value: { path: 'Target', display: 'Follow target' } } } }] } });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.click(screen.getByText('Follow target'));
    expect(mocks.note).toHaveBeenCalledWith('Target', '/kiln');
    expect(mocks.open).not.toHaveBeenCalled();
  });
  it('WS-251: an inline base retains its embedding note as this', () => {
    mocks.query.mockReturnValue({ data: answer() });
    render(() => <BaseView yaml="views: []" host="/kiln/Host.md" />);
    const request = mocks.query.mock.calls[0][0] as () => unknown;
    expect(request()).toMatchObject({ source: { yaml: 'views: []' }, this: '/kiln/Host.md' });
  });
  it('WS-252: a refused new entry is visible instead of looking successful', async () => {
    mocks.query.mockReturnValue({ data: answer() });
    mocks.create.mockRejectedValue(new Error('Folder is missing'));
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.click(screen.getByText('New item'));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Folder is missing'));
  });
  it('WS-252: dragging into the empty group sends a hashed deletion and shows refusal', async () => {
    mocks.query.mockReturnValue({ data: { ...answer(), view_type: 'kanban', group_property: 'note.status', groups: [
      { value: { type: 'null' }, rows: [] },
      { value: { type: 'string', value: 'todo' }, rows: [row] },
    ] } });
    mocks.set.mockRejectedValue(new Error('Entry changed; reload the board'));
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.dragStart(container.querySelector('article')!);
    const empty = screen.getByRole('heading', { name: 'No value 0' }).parentElement!;
    fireEvent.drop(empty);
    await waitFor(() => expect(mocks.set).toHaveBeenCalledWith({ kiln: 'Work', path: 'a.md', key: 'note.status', value: null, delete: true, ancestor_hash: 'h1' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Entry changed'));
  });
  it('renders base fences and named embeds as core mounts, with escaped source', async () => {
    const html = await renderMarkdownDocAsync('```base\nfilters: \'title == "<script>"\'\nviews: []\n```\n\n![[Tasks.base#Board]]');
    expect(html).toContain('class="base-mount"');
    expect(html).toContain('data-base-yaml=');
    expect(html).toContain('data-base-path="Tasks.base"');
    expect(html).toContain('data-base-view="Board"');
    expect(html).not.toContain('<script>');
    expect(html).not.toContain('!<span');
  });
  it('formats lists and preserves the typed empty group for deletion', () => {
    expect(baseText({ type: 'list', value: [{ type: 'number', value: 3 }, { type: 'boolean', value: false }] })).toBe('3, false');
    expect(baseJson({ type: 'null' })).toBeNull();
    expect(baseText({ type: 'link', value: { path: 'a.md', display: 'Alpha' } })).toBe('Alpha');
  });
});
