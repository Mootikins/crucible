import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { BaseView } from '../BaseView';
import { renderMarkdownDocAsync } from '@/lib/markdown';
import { baseText, type BaseGroup } from '@/lib/query/bases';
import { baseOptions } from '@/test-utils/bases';
import { movedGroupOrder } from '../BaseView';
import { mountBases } from '../mount';

const mocks = vi.hoisted(() => ({ query: vi.fn(), set: vi.fn(), create: vi.fn(), open: vi.fn(), note: vi.fn() }));
vi.mock('@/lib/query/kilns', () => ({ useKilns: () => ({ data: [{ name: 'Work', path: '/kiln' }] }) }));
vi.mock('@/lib/query/bases', async importOriginal => ({ ...await importOriginal<object>(), useBase: (...args: unknown[]) => mocks.query(...args), setBaseProperty: (...args: unknown[]) => mocks.set(...args), newBaseEntry: (...args: unknown[]) => mocks.create(...args) }));
vi.mock('@/lib/note-actions', () => ({ openNoteInEditor: (...args: unknown[]) => mocks.note(...args) }));
vi.mock('@/lib/file-actions', () => ({ openFileInEditor: (...args: unknown[]) => mocks.open(...args) }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const row = { path: 'a.md', ancestor_hash: 'h1', movable: true, values: { 'file.name': { type: 'string', value: 'a.md' }, 'note.done': { type: 'boolean', value: false } } };
const answer = () => ({ root: '/kiln', view: 'Table', view_type: 'table', columns: [{ property: 'file.name', display_name: 'Name' }, { property: 'note.done', display_name: 'Done' }], rows: [row], groups: [], summaries: {}, options: baseOptions(), group_property: null, views: [{ name: 'Table', type: 'table' }, { name: 'Cards', type: 'cards' }] });

describe('Bases', () => {
  it('WS-250: renders a typed query, opens its entry, and switches named views', async () => {
    mocks.query.mockReturnValue({ data: answer() });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    expect(screen.getByRole('checkbox', { name: 'false' })).not.toBeChecked();
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
  it('opens file-valued attachments directly', () => {
    mocks.query.mockReturnValue({ data: { ...answer(), rows: [{ ...row, values: { 'file.name': { type: 'file', value: 'cover.png' } } }] } });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.click(screen.getByText('cover.png'));
    expect(mocks.open).toHaveBeenCalledWith('/kiln/cover.png');
    expect(mocks.note).not.toHaveBeenCalled();
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
      { value: { type: 'null' }, write_value: null, rows: [] },
      { value: { type: 'string', value: 'todo' }, write_value: 'todo', rows: [row] },
    ] } });
    mocks.set.mockRejectedValue(Object.assign(new Error('Entry changed; reload the board'), { status: 409 }));
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.dragStart(container.querySelector('article')!);
    const empty = screen.getByRole('heading', { name: 'No value 0' }).parentElement!;
    fireEvent.drop(empty);
    await waitFor(() => expect(mocks.set).toHaveBeenCalledWith({ kiln: 'Work', path: 'a.md', key: 'note.status', delete: true, ancestor_hash: 'h1' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Entry changed'));
    expect(screen.getByRole('alert').textContent).toContain('The entry changed after the view loaded');
  });
  it('sends the write value of the daemon unchanged, with no UTC conversion', async () => {
    mocks.query.mockReturnValue({ data: { ...answer(), view_type: 'kanban', group_property: 'note.due', groups: [
      { value: { type: 'dateonly', value: '2026-09-27' }, write_value: '2026-09-27', rows: [] },
      { value: { type: 'date', value: Date.UTC(2026, 8, 27, 8) }, write_value: '2026-09-27T10:00', rows: [] },
      { value: { type: 'link', value: { path: 'People/Ann.md', display: 'Ann' } }, write_value: '[[People/Ann|Ann]]', rows: [] },
      { value: { type: 'null' }, write_value: null, rows: [row] },
    ] } });
    mocks.set.mockResolvedValue({ status: 'applied', path: 'a.md', ancestor_hash: 'h2' });
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    const headings = screen.getAllByRole('heading');
    for (const [index, value] of [[0, '2026-09-27'], [1, '2026-09-27T10:00'], [2, '[[People/Ann|Ann]]']] as const) {
      fireEvent.dragStart(container.querySelector('article')!);
      fireEvent.drop(headings[index]!.parentElement!);
      await waitFor(() => expect(mocks.set).toHaveBeenLastCalledWith(expect.objectContaining({ key: 'note.due', value })));
    }
  });
  it('a row the daemon marks as not movable cannot be dragged', () => {
    mocks.query.mockReturnValue({ data: { ...answer(), view_type: 'kanban', group_property: 'note.status', groups: [
      { value: { type: 'string', value: 'a' }, write_value: 'a', rows: [{ ...row, movable: false }] },
    ] } });
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    expect(container.querySelector('article')!.getAttribute('draggable')).toBe('false');
  });
  it('an unchanged write says that nothing changed', async () => {
    mocks.query.mockReturnValue({ data: answer() });
    mocks.create.mockResolvedValue({ status: 'unchanged', path: 'a.md', ancestor_hash: 'h1' });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.click(screen.getByText('New item'));
    await waitFor(() => expect(screen.getByText('The note already had this value. Nothing changed.')).toBeTruthy());
  });
  it('a proposed write says that it waits in the Inbox', async () => {
    mocks.query.mockReturnValue({ data: answer() });
    mocks.create.mockResolvedValue({ status: 'proposed', path: 'b.md', proposal: 'p1' });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    fireEvent.click(screen.getByText('New item'));
    await waitFor(() => expect(screen.getByText('The change waits for review in the Inbox.')).toBeTruthy());
  });
  it('honors card image options and renders typed fields instead of stringifying them', () => {
    mocks.query.mockReturnValue({ data: { ...answer(), view_type: 'cards', options: baseOptions({ card_size: 310, image: 'note.cover', image_fit: 'contain', image_aspect_ratio: 1.5 }), rows: [{ ...row, values: { ...row.values, 'note.cover': { type: 'string', value: 'cover.png' } } }] } });
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    const image = screen.getByRole('img', { name: 'Entry image' });
    expect(image.getAttribute('src')).toContain('cover.png');
    expect(image.style.objectFit).toBe('contain');
    expect(image.style.getPropertyValue('aspect-ratio')).toBe('1.5 / 1');
    expect(container.querySelector('[data-base-cards]')?.getAttribute('style')).toContain('310px');
    expect(screen.getByRole('checkbox')).toBeDisabled();
  });
  it('honors list markers and indentation and displays group summaries', () => {
    mocks.query.mockReturnValue({ data: { ...answer(), view_type: 'list', options: baseOptions({ markers: 'number', indent_properties: true, separator: ' / ' }), groups: [{ value: { type: 'string', value: 'todo' }, rows: [row], summaries: { Count: { type: 'number', value: 1 } } }] } });
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    expect(container.querySelector('ol')).toBeTruthy();
    expect(container.querySelector('[data-base-properties]')?.getAttribute('class')).toContain('ml-4');
    expect(screen.getByText('Count')).toBeTruthy();
  });
  it('draws the kanban columns the daemon sends at the configured width', () => {
    // The daemon removes empty columns itself when the view hides them.
    mocks.query.mockReturnValue({ data: { ...answer(), view_type: 'kanban', group_property: 'note.status', options: baseOptions({ hide_empty_groups: true, column_width: 410 }), groups: [{ value: { type: 'string', value: 'todo' }, write_value: 'todo', rows: [row], summaries: {} }] } });
    render(() => <BaseView filePath="/kiln/Tasks.base" />);
    expect(screen.getAllByRole('heading')).toHaveLength(1);
    expect(screen.getByRole('heading', { name: 'todo 1' }).parentElement?.style.width).toBe('410px');
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
  it('renders a typed link label', () => {
    mocks.query.mockReturnValue({ data: { ...answer(), rows: [{ ...row, values: { ...row.values, 'file.name': { type: 'link', value: { path: 'a.md', display: 'true', display_value: { type: 'boolean', value: true } } } } }] } });
    const { container } = render(() => <BaseView filePath="/kiln/Tasks.base" />);
    expect(container.querySelector('a input[type="checkbox"]')).toBeTruthy();
  });
  it('an embed with no view name asks for the first view, not a view named ""', async () => {
    mocks.query.mockReturnValue({ data: answer() });
    const html = await renderMarkdownDocAsync('![[Tasks.base]]');
    expect(html).not.toContain('data-base-view');
    const host = document.createElement('div');
    host.innerHTML = html;
    // A placeholder that still carries an empty attribute means the same.
    host.insertAdjacentHTML('beforeend', '<span class="base-mount" data-base-path="Tasks.base" data-base-view=""></span>');
    const dispose = mountBases(host, '/kiln/Host.md', 'Work');
    try {
      expect(mocks.query).toHaveBeenCalledTimes(2);
      for (const [request] of mocks.query.mock.calls as [() => { view?: string }][]) expect(request().view).toBeUndefined();
    } finally {
      dispose();
    }
  });
  it('a column dragged right can reach the last position', () => {
    const group = (write_value: string | null): BaseGroup => ({ value: write_value === null ? { type: 'null' } : { type: 'string', value: write_value }, write_value, rows: [], summaries: {} });
    const [a, b, c, none] = [group('A'), group('B'), group('C'), group(null)];
    const text = (order: unknown[]) => order;
    expect(text(movedGroupOrder([a, b, c, none], a, c))).toEqual(['B', 'C', 'A']);
    expect(text(movedGroupOrder([a, b, c], a, b))).toEqual(['B', 'A', 'C']);
    expect(text(movedGroupOrder([a, b, c], c, a))).toEqual(['C', 'A', 'B']);
    expect(text(movedGroupOrder([a, b, c], c, b))).toEqual(['A', 'C', 'B']);
  });
  it('shows a regular expression as its source', () => {
    expect(baseText({ type: 'regexp', value: { pattern: 'a+b', flags: 'gi' } })).toBe('/a+b/gi');
  });
  it('formats lists, links, daemon-worded durations and cell errors', () => {
    expect(baseText({ type: 'list', value: [{ type: 'number', value: 3 }, { type: 'boolean', value: false }] })).toBe('3, false');
    expect(baseText({ type: 'link', value: { path: 'a.md', display: 'Alpha' } })).toBe('Alpha');
    expect(baseText({ type: 'duration', value: { milliseconds: 3_600_000, months: 0, text: 'an hour' } })).toBe('an hour');
    expect(baseText({ type: 'error', value: 'bad formula' })).toBe('Error: bad formula');
  });
});
