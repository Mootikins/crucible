import { DataTable } from '../DataTable';
import { sanitizeDocHtml } from '@/lib/markdown';
import { rawFileUrl } from '@/lib/paths';
import { createEffect, on, createMemo, createSignal, For, Show, type Component } from 'solid-js';
import { useKilns } from '@/lib/query/kilns';
import { useBase, setBaseGroupOrder, setBaseProperty, newBaseEntry, baseText, baseJson, type BaseRow, type BaseValue } from '@/lib/query/bases';
import { openFileInEditor } from '@/lib/file-actions';
import { openNoteInEditor } from '@/lib/note-actions';
import { isMarkdownPath } from '@/lib/markdown-path';

export const BaseView: Component<{ filePath?: string; yaml?: string; host?: string; kiln?: string; view?: string }> = (props) => {
  const kilns = useKilns();
  const [view, setView] = createSignal<string>();
  createEffect(on(() => [props.filePath, props.yaml, props.view], () => setView(undefined), { defer: true }));
  const [error, setError] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [drag, setDrag] = createSignal<BaseRow>();
  const [columnDrag, setColumnDrag] = createSignal<BaseValue>();
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
  async function act(action: () => Promise<unknown>) {
    setBusy(true); setError('');
    try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  const open = (row: BaseRow) => openFileInEditor(`${result.data!.root}/${row.path}`);
  const canMove = (row: BaseRow) => result.data?.group_property === 'file.folder' || (isMarkdownPath(row.path) && !!result.data?.group_property && !/^(file|formula)\./.test(result.data.group_property));
  const reorder = (order: unknown[] | null) => act(() => setBaseGroupOrder({ ...request(), view: result.data!.view, group_order: order, ancestor_hash: result.data!.source_hash }));
  function dropColumn(target: BaseValue) {
    const from = columnDrag(); setColumnDrag(undefined);
    if (!from || target.type === 'null' || from === target) return;
    const same = (a: BaseValue, b: BaseValue) => JSON.stringify(baseJson(a)) === JSON.stringify(baseJson(b));
    const order = result.data!.groups.map(g => g.value).filter(v => !same(v, from) && v.type !== 'null');
    order.splice(order.findIndex(value => same(value,target)), 0, from);
    void reorder(order.map(baseJson));
  }
  const create = (group?: BaseValue) => act(() => newBaseEntry({ ...request(), name: name() || undefined, ...(group ? { group: baseJson(group) } : {}) }));
  const move = (row: BaseRow, value: BaseValue) => act(() => setBaseProperty({ kiln: kiln()!.name, path: row.path,
    key: result.data!.group_property, value: baseJson(value), delete: value.type === 'null', ancestor_hash: row.ancestor_hash }));
  const cards = (rows: BaseRow[]) => <For each={rows}>{row => <article class="rounded border border-hairline bg-surface-base p-3" draggable={canMove(row)} onDragStart={() => setDrag(row)} onDragEnd={() => setDrag(undefined)}>
    <button class="font-medium text-primary hover:underline" onClick={() => open(row)}>{baseText(row.values[result.data!.columns[0]?.property]) || row.path}</button>
    <For each={result.data!.columns.slice(1)}>{column => <p class="text-sm"><span class="text-muted">{column.display_name}: </span>{baseText(row.values[column.property])}</p>}</For>
  </article>}</For>;
  const cell = (value: BaseValue | undefined) => {
    if (value?.type === 'html') return <span innerHTML={sanitizeDocHtml(String(value.value))} />;
    if (value?.type === 'image') {
      const path = String(value.value);
      return <img class="max-h-40 max-w-full" alt="" src={/^https?:\/\//i.test(path) ? path : rawFileUrl(`${result.data!.root}/${path}`)} />;
    }
    if (value?.type === 'link') {
      const link = value.value as { path: string; display?: string };
      if (/^https?:\/\//i.test(link.path)) return <a href={link.path} target="_blank" rel="noopener noreferrer">{link.display ?? link.path}</a>;
      return <a href="#" class="wikilink" data-note={link.path} onClick={event => { event.preventDefault(); event.stopPropagation(); void openNoteInEditor(link.path, result.data!.root); }}>{link.display ?? link.path}</a>;
    }
    return baseText(value);
  };
  const table = (rows: BaseRow[]) => <DataTable columns={result.data!.columns.map(c => ({ key: c.property, title: c.display_name }))} rows={rows} cell={(row,key,index) => index === 0 && !['link', 'html', 'image'].includes(row.values[key]?.type ?? '') ? <button class="text-primary hover:underline" onClick={() => open(row)}>{cell(row.values[key]) || row.path}</button> : cell(row.values[key])} />;
  const renderRows = (rows: BaseRow[]) => result.data?.view_type === 'cards' ? <div class="grid grid-cols-2 gap-3">{cards(rows)}</div>
    : result.data?.view_type === 'list' ? <ul class="space-y-2"><For each={rows}>{row => <li><button class="text-primary hover:underline" onClick={() => open(row)}>{result.data!.columns.map(c => baseText(row.values[c.property])).join(' · ')}</button></li>}</For></ul> : table(rows);
  return <section class="base-view p-3 space-y-3" aria-label="Base view" data-kiln={kiln()?.path}>
    <Show when={kilns.isPending || result.isLoading}><p role="status">Loading base…</p></Show>
    <Show when={!kilns.isPending && !kiln()}><p role="alert">This base needs a registered kiln.</p></Show>
    <Show when={error() || result.error}><p role="alert" class="text-error">{error() || result.error?.message}</p></Show>
    <Show when={result.data}>{data => <>
      <div class="flex flex-wrap items-center gap-2">
        <select aria-label="Base view" class="bg-surface-base border border-hairline rounded p-1" value={data().view} onChange={e => setView(e.currentTarget.value)}><For each={data().views}>{v => <option value={v.name}>{v.name}</option>}</For></select>
        <Show when={data().source_hash && data().group_property}><button disabled={busy()} onClick={() => reorder(null)}>Reset columns</button></Show>
        <span class="text-muted text-sm">{data().rows.length} {data().rows.length === 1 ? 'entry' : 'entries'}</span>
        <input aria-label="New entry name" placeholder="New entry name" class="bg-surface-base border border-hairline rounded p-1" value={name()} onInput={e => setName(e.currentTarget.value)} />
        <button disabled={busy()} onClick={() => create()}>New item</button>
      </div>
      <Show when={data().view_type === 'kanban'} fallback={data().groups.length ? <For each={data().groups}>{g => <section><h3 class="font-medium">{baseText(g.value) || 'No value'}</h3>{renderRows(g.rows)}</section>}</For> : renderRows(data().rows)}>
        <Show when={data().group_property} fallback={<p>Select a groupBy property in the base source to use Kanban.</p>}>
          <div class="flex gap-3 overflow-x-auto"><For each={data().groups}>{group => <section class="min-w-64 w-72 shrink-0 space-y-2 rounded bg-surface-elevated p-2" onDragOver={e => { if (drag() || columnDrag()) e.preventDefault(); }} onDrop={e => { e.preventDefault(); if (columnDrag()) { dropColumn(group.value); return; } const row = drag(); setDrag(undefined); if (row) void move(row, group.value); }}>
            <h3 class="font-medium" draggable={!!data().source_hash && group.value.type !== 'null'} onDragStart={() => setColumnDrag(group.value)} onDragEnd={() => setColumnDrag(undefined)}>{baseText(group.value) || 'No value'} <span class="text-muted">{group.rows.length}</span></h3>
            {cards(group.rows)}<button disabled={busy()} onClick={() => create(group.value)}>+ New item</button>
          </section>}</For></div>
        </Show>
      </Show>
      <Show when={Object.keys(data().summaries).length}><dl class="flex flex-wrap gap-3 text-sm"><For each={Object.entries(data().summaries)}>{([key,value]) => <div><dt class="text-muted">{key}</dt><dd>{baseText(value)}</dd></div>}</For></dl></Show>
      <Show when={!['table','list','cards','kanban'].includes(data().view_type)}><p class="text-muted">View type “{data().view_type}” is displayed as a table.</p></Show>
    </>}</Show>
  </section>;
};
