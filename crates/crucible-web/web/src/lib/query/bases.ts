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
  if (value.type === 'duration') return durationText(value.value.milliseconds + value.value.months * 30.436875 * 86400000);
  if (value.type === 'relativedate') {
    const delta = value.value - Date.now();
    const text = durationText(delta);
    return delta > 0 ? `in ${text}` : `${text} ago`;
  }
  if (value.type === 'dateonly') return value.value;
  if (value.type === 'date') return new Date(value.value as number).toLocaleString();
  if (value.type === 'object') return JSON.stringify(value.value);
  return String(value.value ?? '');
}
export function baseJson(value: BaseValue): unknown {
  if (value.type === 'null') return null;
  if (value.type === 'list') return (value.value as BaseValue[]).map(baseJson);
  if (value.type === 'link') return `[[${value.value.path}${value.value.display == null ? '' : `|${value.value.display}`}]]`;
  if (value.type === 'date' || value.type === 'relativedate') return new Date(value.value).toISOString();
  return value.value;
}

export async function setBaseGroupOrder(request: Record<string, unknown>) {
  const answer = await reorderBaseGroups(request);
  await getQueryClient().invalidateQueries({ queryKey: keys.bases() });
  return answer;
}

function durationText(milliseconds: number): string {
  const seconds = Math.round(Math.abs(milliseconds) / 1000);
  const minutes = Math.round(seconds / 60), hours = Math.round(minutes / 60), days = Math.round(hours / 24);
  if (seconds < 45) return 'a few seconds';
  if (seconds < 90) return 'a minute';
  if (minutes < 45) return `${minutes} minutes`;
  if (minutes < 90) return 'an hour';
  if (hours < 22) return `${hours} hours`;
  if (hours < 36) return 'a day';
  if (days < 26) return `${days} days`;
  if (days < 46) return 'a month';
  if (days < 320) return `${Math.round(days / 30.436875)} months`;
  if (days < 548) return 'a year';
  return `${Math.round(days / 365.2425)} years`;
}
