import { For, type Component, type JSX } from 'solid-js';
import { sanitizeDocHtml } from '@/lib/markdown';
import { rawFileUrl } from '@/lib/paths';
import { openFileInEditor } from '@/lib/file-actions';
import { openNoteInEditor } from '@/lib/note-actions';
import { baseText, type BaseValue } from '@/lib/query/bases';
import { BaseIcon } from './BaseIcon';

export const BaseCell: Component<{ value?: BaseValue; root: string }> = props => {
  const content = (): JSX.Element => {
    const value = props.value;
    if (!value || value.type === 'null') return '';
    if (value.type === 'boolean') return <input type="checkbox" checked={value.value} disabled aria-label={String(value.value)} />;
    if (value.type === 'list') return <For each={value.value}>{(v, index) => <>{index() ? ', ' : ''}<BaseCell value={v} root={props.root} /></>}</For>;
    if (value.type === 'html') return <span innerHTML={sanitizeDocHtml(value.value)} />;
    if (value.type === 'icon') return <BaseIcon name={value.value} />;
    if (value.type === 'image') return <img class="max-h-40 max-w-full" alt="Entry image" src={baseImageUrl(value.value, props.root)} />;
    if (value.type === 'link' || value.type === 'file') {
      const link: { path: string; display?: string | null; display_value?: BaseValue | null } = value.type === 'file' ? { path: value.value } : value.value;
      const label = link.display_value ? <BaseCell value={link.display_value} root={props.root} /> : link.display ?? link.path;
      if (/^https?:\/\//i.test(link.path)) return <a href={link.path} target="_blank" rel="noopener noreferrer">{label}</a>;
      return <a href="#" class="wikilink text-primary" data-note={link.path} onClick={event => { event.preventDefault(); event.stopPropagation(); if (value.type === 'file') openFileInEditor(`${props.root}/${link.path}`); else void openNoteInEditor(link.path, props.root); }}>{label}</a>;
    }
    return baseText(value);
  };
  return <>{content()}</>;
};
export function baseImageUrl(path: string, root: string): string {
  return /^https?:\/\//i.test(path) ? path : rawFileUrl(`${root}/${path}`);
}
