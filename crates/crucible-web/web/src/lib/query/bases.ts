import type { components } from '../api-schema';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { getQueryClient } from './client';
import { keys } from './keys';
import { queryBase, writeBaseProperty, createBaseEntry, reorderBaseGroups } from '../api';

export type BaseValue = components['schemas']['BaseValue'];
export type BaseRow = components['schemas']['Row'];
export interface BaseRequest { kiln: string; source: { path: string } | { yaml: string }; view?: string; this?: string }
export function useBase(request: Accessor<BaseRequest | null>) {
  return useQuery(() => ({ queryKey: keys.baseQuery(request()), enabled: request() !== null,
    queryFn: () => queryBase(request()!), }), getQueryClient);
}
export async function setBaseProperty(request: Record<string, unknown>) {
  const answer = await writeBaseProperty(request);
  await getQueryClient().invalidateQueries({ queryKey: keys.bases() });
  return answer;
}
export async function newBaseEntry(request: Record<string, unknown>) {
  const answer = await createBaseEntry(request);
  await getQueryClient().invalidateQueries({ queryKey: keys.bases() });
  return answer;
}
export function baseText(value: BaseValue | undefined): string {
  if (!value || value.type === 'null') return '';
  if (value.type === 'list') return (value.value as BaseValue[]).map(baseText).join(', ');
  if (value.type === 'link') { const v = value.value as { path: string; display?: string }; return v.display ?? v.path; }
  if (value.type === 'date') return new Date(value.value as number).toLocaleString();
  if (value.type === 'object') return JSON.stringify(value.value);
  return String(value.value ?? '');
}
export function baseJson(value: BaseValue): unknown {
  if (value.type === 'null') return null;
  if (value.type === 'list') return (value.value as BaseValue[]).map(baseJson);
  return value.value;
}

export async function setBaseGroupOrder(request: Record<string, unknown>) {
  const answer = await reorderBaseGroups(request);
  await getQueryClient().invalidateQueries({ queryKey: keys.bases() });
  return answer;
}
