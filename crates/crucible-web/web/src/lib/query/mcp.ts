import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import { getMcpStatus } from '@/lib/api';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * What the daemon reports about its MCP connections.
 *
 * The settings pane read this in an `onMount` and kept it in three signals of
 * its own — the value, a loading flag and an error string — which is the shape
 * a query already has. It also meant that closing the settings dialog and
 * opening it again asked the daemon over.
 *
 * `staleTime` is a minute rather than the five the app defaults to. Nothing
 * pushes a change on this: the MCP server can start or stop under a browser
 * that stays open, and the pane is where a user goes to find out. A minute is
 * short enough that re-opening the pane after a restart tells the truth.
 */
export function useMcpStatus(): UseQueryResult<Record<string, unknown>, Error> {
  return useQuery(
    () => ({
      queryKey: keys.mcpStatus(),
      queryFn: getMcpStatus,
      staleTime: 60_000,
    }),
    getQueryClient,
  );
}
