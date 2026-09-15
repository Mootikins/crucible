// src/components/settings/McpStatus.tsx
//
// What the daemon reports about its MCP connections, as key/value rows.
import { Component, For, createSignal, onMount } from 'solid-js';
import { Link2 } from '@/lib/icons';

import { SettingsSectionState } from './primitives';
import { getMcpStatus } from '@/lib/api';

export const McpStatusSection: Component = () => {
  const [status, setStatus] = createSignal<Record<string, unknown> | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  const loadStatus = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getMcpStatus();
      setStatus(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load MCP status');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadStatus);

  return (
    <SettingsSectionState
      title="MCP Status"
      icon={Link2}
      loading={loading()}
      error={error()}
      loadingMessage="Loading MCP status…"
      onRetry={loadStatus}
      hideContentOnError
      isEmpty={!status()}
      emptyMessage="No MCP status available."
    >
      <For each={Object.entries(status()!)}>
        {([key, value]) => (
          <tr class="border-b border-hairline">
            <td class="py-2.5 text-shell-body text-sm">{key}</td>
            <td class="py-2.5 text-right text-sm text-muted max-w-[200px] truncate">
              {typeof value === 'object' ? JSON.stringify(value) : String(value ?? '—')}
            </td>
          </tr>
        )}
      </For>
    </SettingsSectionState>
  );
};
