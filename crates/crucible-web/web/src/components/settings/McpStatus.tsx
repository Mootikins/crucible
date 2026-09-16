// src/components/settings/McpStatus.tsx
//
// What the daemon reports about its MCP connections, as key/value rows.
import { Component, For } from 'solid-js';
import { Link2 } from '@/lib/icons';

import { SettingsSectionState } from './primitives';
import { useMcpStatus } from '@/lib/query/mcp';

export const McpStatusSection: Component = () => {
  // The three signals this section kept — a value, a loading flag and an error
  // string — are the shape a query already has, and the pane asked the daemon
  // again every time the dialog opened.
  const status = useMcpStatus();

  return (
    <SettingsSectionState
      title="MCP Status"
      icon={Link2}
      loading={status.isPending}
      error={status.error?.message ?? null}
      loadingMessage="Loading MCP status…"
      onRetry={() => void status.refetch()}
      hideContentOnError
      isEmpty={!status.data}
      emptyMessage="No MCP status available."
    >
      <For each={Object.entries(status.data ?? {})}>
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
