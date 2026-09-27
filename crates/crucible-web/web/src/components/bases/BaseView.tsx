import { DataTable } from '../DataTable';
import { BaseCell, baseImageUrl } from './BaseCell';
import { Dynamic } from 'solid-js/web';
import { createEffect, on, createMemo, createSignal, For, Show, type Component, type JSX } from 'solid-js';
import { useKilns } from '@/lib/query/kilns';
import { useBase, setBaseGroupOrder, setBaseProperty, newBaseEntry, baseText, baseWriteError, baseOutcomeNotice, type BaseGroup, type BaseRow, type BaseValue, type WriteOutcome } from '@/lib/query/bases';
import { openFileInEditor } from '@/lib/file-actions';

export interface BaseViewProps {
  filePath?: string;
  yaml?: string;
  host?: string;
  kiln?: string;
  view?: string;
}

/** The element and the marker style of each list marker option. */
const LIST_MARKERS: Record<string, { tag: 'ol' | 'ul'; style: string }> = {
  number: { tag: 'ol', style: 'decimal' },
  none: { tag: 'ul', style: 'none' },
  bullet: { tag: 'ul', style: 'disc' },
};

/** The pixel height of each row height option. Other values use the compact height. */
const ROW_HEIGHTS: Record<string, number> = { medium: 56, tall: 112, extra: 224 };
const COMPACT_ROW_HEIGHT = 28;

/** Two groups are one group when they write the same value. */
const sameGroup = (a: BaseGroup, b: BaseGroup) => JSON.stringify(a.write_value) === JSON.stringify(b.write_value);

/**
 * The group order after a drag of the column `from` onto the column `target`,
 * as the values the daemon gave each group to write. The group with no value
 * has no place in the order. A column dragged to the right lands after its
 * target, so it can reach the last position; a column dragged to the left
 * lands before its target.
 */
export function movedGroupOrder(groups: readonly BaseGroup[], from: BaseGroup, target: BaseGroup): unknown[] {
  const valued = groups.filter(g => g.write_value !== null);
  const fromIndex = valued.findIndex(g => sameGroup(g, from));
  const targetIndex = valued.findIndex(g => sameGroup(g, target));
  const order = valued.filter(g => !sameGroup(g, from));
  const at = order.findIndex(g => sameGroup(g, target));
  order.splice(fromIndex >= 0 && targetIndex > fromIndex ? at + 1 : at, 0, from);
  return order.map(g => g.write_value);
}

export const BaseView: Component<BaseViewProps> = (props) => {
  const kilns = useKilns();
  const [view, setView] = createSignal<string>();
  createEffect(on(() => [props.filePath, props.yaml, props.view], () => setView(undefined), { defer: true }));
  const [error, setError] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [drag, setDrag] = createSignal<BaseRow>();
  const [columnDrag, setColumnDrag] = createSignal<BaseGroup>();
  const [notice, setNotice] = createSignal('');
  const [name, setName] = createSignal('');
  const kiln = createMemo(() => {
    if (props.kiln) return kilns.data?.find(k => k.name === props.kiln || k.path === props.kiln);
    const path = props.host ?? props.filePath;
    return kilns.data?.filter(k => path?.startsWith(`${k.path}/`)).sort((a,b) => b.path.length - a.path.length)[0];
  });
  const request = createMemo(() => kiln()?.name ? {
    kiln: kiln()!.name, source: props.yaml !== undefined ? { yaml: props.yaml } : { path: props.filePath ?? '' },
    view: view() ?? props.view, this: props.host,
  } : null);
  const result = useBase(request);
  const options = () => result.data!.options;
  /** The request of a write: an inline base writes back to the saved base the daemon names, when it names one. */
  const mutationRequest = () => {
    const current = request()!;
    return { ...current, source: result.data?.source_path ? { path: result.data.source_path } : current.source };
  };

  async function act(action: () => Promise<WriteOutcome>) {
    setBusy(true); setError(''); setNotice('');
    try { setNotice(baseOutcomeNotice(await action()) ?? ''); } catch (e) { setError(baseWriteError(e)); }
    finally { setBusy(false); }
  }
  const open = (row: BaseRow) => openFileInEditor(`${result.data!.root}/${row.path}`);
  const reorder = (order: unknown[] | null) => act(() => setBaseGroupOrder({ ...mutationRequest(), view: result.data!.view, group_order: order, ancestor_hash: result.data!.source_hash ?? '' }));
  function dropColumn(target: BaseGroup) {
    const from = columnDrag(); setColumnDrag(undefined);
    if (!from || target.write_value === null || sameGroup(from, target)) return;
    void reorder(movedGroupOrder(result.data!.groups, from, target));
  }
  // The daemon decides what a group writes and which rows can move; the
  // browser sends its `write_value` unchanged.
  const create = (group?: BaseGroup) => act(() => newBaseEntry({ ...mutationRequest(), name: name() || undefined, ...(group ? { group: group.write_value } : {}) }));
  const move = (row: BaseRow, group: BaseGroup) => act(() => setBaseProperty({ kiln: kiln()!.name, path: row.path,
    key: result.data!.group_property ?? '', ...(group.write_value === null ? { delete: true } : { value: group.write_value }), ancestor_hash: row.ancestor_hash }));
  const cell = (value: BaseValue | undefined) => <BaseCell value={value} root={result.data!.root} />;
  const title = (row: BaseRow) => {
    const value = row.values[result.data!.columns[0]?.property];
    return ['link', 'file', 'html', 'image', 'boolean', 'list'].includes(value?.type ?? '') ? cell(value)
      : <button class="font-medium text-primary hover:underline" onClick={() => open(row)}>{cell(value)}{!baseText(value) ? row.path : ''}</button>;
  };
  const summaries = (values: Record<string, BaseValue> | undefined) => <Show when={values && Object.keys(values).length}><dl class="flex flex-wrap gap-3 text-sm"><For each={Object.entries(values ?? {})}>{([key,value]) => <div><dt class="text-muted">{key}</dt><dd>{cell(value)}</dd></div>}</For></dl></Show>;
  const cover = (row: BaseRow) => {
    const property = options().image;
    let value = property ? row.values[property] : undefined;
    if (value?.type === 'list') value = value.value[0];
    const path = value?.type === 'link' ? value.value.path : value && (value.type === 'string' || value.type === 'file' || value.type === 'image') ? value.value : undefined;
    return path ? <img alt="Entry image" class="w-full" style={{ "object-fit": options().image_fit === 'contain' ? 'contain' : 'cover', "aspect-ratio": `${options().image_aspect_ratio} / 1` }} src={baseImageUrl(path, result.data!.root)} /> : null;
  };
  const cards = (rows: BaseRow[]) => <For each={rows}>{row => <article class="rounded border border-hairline bg-surface-base p-3 min-w-0" draggable={row.movable} onDragStart={() => setDrag(row)} onDragEnd={() => setDrag(undefined)}>
    {cover(row)}{title(row)}
    <For each={result.data!.columns.slice(1)}>{column => <div class="text-sm break-words"><span class="text-muted">{column.display_name}: </span>{cell(row.values[column.property])}</div>}</For>
  </article>}</For>;
  const table = (rows: BaseRow[]) => <DataTable columns={result.data!.columns.map(c => ({ key: c.property, title: c.display_name, width: options().column_size[c.property] }))} rowHeight={ROW_HEIGHTS[options().row_height] ?? COMPACT_ROW_HEIGHT} rows={rows} cell={(row,key,index) => index === 0 ? title(row) : cell(row.values[key])} />;
  const list = (rows: BaseRow[]) => {
    const markers = LIST_MARKERS[options().markers] ?? LIST_MARKERS.bullet;
    const indent = options().indent_properties;
    return <Dynamic component={markers.tag} class="space-y-2 pl-5" style={{ "list-style-type": markers.style }}><For each={rows}>{row => <li>
      {title(row)}<span data-base-properties class={indent ? 'block ml-4' : ''}><For each={result.data!.columns.slice(1)}>{column => <span class={indent ? 'block' : ''}>{indent ? '' : options().separator}{cell(row.values[column.property])}</span>}</For></span>
    </li>}</For></Dynamic>;
  };
  const renderRows = (rows: BaseRow[]): JSX.Element => {
    switch (result.data?.view_type) {
      case 'cards':
        return <div data-base-cards class="grid gap-3" style={{ "grid-template-columns": `repeat(auto-fill, minmax(min(100%, ${options().card_size}px), 1fr))` }}>{cards(rows)}</div>;
      case 'list':
        return list(rows);
      default:
        return table(rows);
    }
  };
  return <section class="base-view p-3 space-y-3" aria-label="Base view" data-kiln={kiln()?.path}>
    <Show when={kilns.isPending || result.isLoading}><p role="status">Loading base…</p></Show>
    <Show when={!kilns.isPending && !kiln()}><p role="alert">This base needs a registered kiln.</p></Show>
    <Show when={notice()}><p role="status">{notice()}</p></Show>
    <Show when={error() || result.error}><p role="alert" class="text-error">{error() || result.error?.message}</p></Show>
    <Show when={result.data}>{data => <>
      <div class="flex flex-wrap items-center gap-2">
        <select aria-label="Base view" class="bg-surface-base border border-hairline rounded p-1" value={data().view} onChange={e => setView(e.currentTarget.value)}><For each={data().views}>{v => <option value={v.name}>{v.name}</option>}</For></select>
        <Show when={data().source_hash && data().group_property}><button disabled={busy()} onClick={() => reorder(null)}>Reset columns</button></Show>
        <span class="text-muted text-sm">{data().rows.length} {data().rows.length === 1 ? 'entry' : 'entries'}</span>
        <input aria-label="New entry name" placeholder="New entry name" class="bg-surface-base border border-hairline rounded p-1" value={name()} onInput={e => setName(e.currentTarget.value)} />
        <button disabled={busy()} onClick={() => create()}>New item</button>
      </div>
      <Show when={data().view_type === 'kanban'} fallback={data().groups.length ? <For each={data().groups}>{g => <section><h3 class="font-medium">{baseText(g.value) || 'No value'}</h3>{renderRows(g.rows)}{summaries(g.summaries)}</section>}</For> : renderRows(data().rows)}>
        <Show when={data().group_property} fallback={<p>Select a groupBy property in the base source to use Kanban.</p>}>
          <div class="flex gap-3 overflow-x-auto"><For each={data().groups}>{group => <section class="shrink-0 space-y-2 rounded bg-surface-elevated p-2" style={{ width: `${options().column_width}px` }} onDragOver={e => { if (drag() || columnDrag()) e.preventDefault(); }} onDrop={e => { e.preventDefault(); if (columnDrag()) { dropColumn(group); return; } const row = drag(); setDrag(undefined); if (row) void move(row, group); }}>
            <h3 class="font-medium" draggable={!!data().source_hash && group.write_value !== null} onDragStart={() => setColumnDrag(group)} onDragEnd={() => setColumnDrag(undefined)}>{baseText(group.value) || 'No value'} <span class="text-muted">{group.rows.length}</span></h3>
            {cards(group.rows)}{summaries(group.summaries)}<button disabled={busy()} onClick={() => create(group)}>+ New item</button>
          </section>}</For></div>
        </Show>
      </Show>
      {summaries(data().summaries)}
      <Show when={!['table','list','cards','kanban'].includes(data().view_type)}><p class="text-muted">View type “{data().view_type}” is displayed as a table.</p></Show>
    </>}</Show>
  </section>;
};
