import { For, type JSX } from 'solid-js';

/** Shared table presentation for daemon results and plugin publications. */
export function DataTable<T>(props: { columns: { key: string; title: string; width?: number }[]; rows: T[]; rowHeight?: number; cell: (row: T, key: string, index: number) => JSX.Element }) {
  return <div class="overflow-x-auto"><table class="w-full text-sm text-left"><thead><tr><For each={props.columns}>{column => <th style={{ width: column.width ? `${column.width}px` : undefined, "min-width": column.width ? `${column.width}px` : undefined }} class="border-b border-hairline px-2 py-1 font-medium text-muted">{column.title}</th>}</For></tr></thead><tbody><For each={props.rows}>{row => <tr><For each={props.columns}>{(column,index) => <td style={{ height: props.rowHeight ? `${props.rowHeight}px` : undefined }} class="border-b border-hairline px-2 py-1 align-top">{props.cell(row,column.key,index())}</td>}</For></tr>}</For></tbody></table></div>;
}
