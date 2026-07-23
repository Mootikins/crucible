import { Component, Show, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useChatSafe } from '@/contexts/ChatContext';
import {
  connectSessionKiln,
  disconnectSessionKiln,
  listKilns,
  listProjects,
  setSessionWorkspace,
} from '@/lib/api';
import type { KilnListEntry, Project } from '@/lib/types';
import { notificationActions } from '@/stores/notificationStore';
import { pathBasename } from '@/stores/statusBarStore';
import { kilnLabel } from '@/lib/kiln-label';
import { swrLocal } from '@/lib/local-cache';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import { FlaskConical, FolderGit2 } from '@/lib/icons';

/**
 * Interactive session-scope strip below the chat input: the workspace the
 * session acts in and the kilns it knows. Same ChipSelect popout idiom as the
 * launchpad composer (project = FolderGit2, kiln = FlaskConical) so the two
 * surfaces read identically. Attach/detach mid-session — the daemon rejects
 * mutations mid-turn, re-checks trust on attach, and rebuilds the agent's
 * tools/prompt on the next turn.
 */
export const SessionScopeChips: Component = () => {
  const { currentSession, applySessionScope } = useSessionSafe();
  const { isStreaming } = useChatSafe();

  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [busy, setBusy] = createSignal(false);

  const session = () => currentSession();
  // Workspace == kiln is the daemon's "no workspace" state (Session::new).
  const hasWorkspace = () => {
    const s = session();
    return !!s && s.workspace !== s.kiln;
  };
  const disabled = () => busy() || isStreaming();

  onMount(() => {
    // Last-known values paint instantly on reload (same as the composer).
    swrLocal('kilns', listKilns, setKilns);
    swrLocal('projects', () => listProjects(), setProjects);
  });

  const mutate = async (action: () => Promise<Parameters<typeof applySessionScope>[0]>) => {
    if (disabled()) return;
    setBusy(true);
    try {
      applySessionScope(await action());
    } catch (err) {
      notificationActions.addNotification(
        'error',
        err instanceof Error ? err.message : 'Failed to update session scope',
      );
    } finally {
      setBusy(false);
    }
  };

  // ---- workspace (project) chip -------------------------------------------
  const projectOptions = (): ChipOption[] => [
    // No project → the session keeps its own scratch folder.
    { value: '', label: 'Session folder', hint: 'unique per session', group: 'Projects' },
    ...projects().map((p) => ({
      value: p.path,
      label: p.name || pathBasename(p.path) || p.path,
      hint: p.path,
      group: 'Projects',
    })),
  ];

  const pickProject = (path: string) => {
    const s = session();
    if (!s) return;
    void mutate(() => setSessionWorkspace(s.id, path || null));
  };

  const projectLabel = () => {
    const s = session();
    if (!s || !hasWorkspace()) return 'Session folder';
    return pathBasename(s.workspace ?? '') || 'Session folder';
  };

  // ---- kiln chip (primary + connected, multi-toggle) ----------------------
  const primaryKiln = () => session()?.kiln ?? '';
  const connectedKilns = () => session()?.connected_kilns ?? [];
  const selectedKilns = () => [primaryKiln(), ...connectedKilns()];

  const kilnOptions = (): ChipOption[] => {
    const primary = primaryKiln();
    const connected = new Set(connectedKilns());
    const rows: ChipOption[] = [
      // Primary is where the session is stored — locked, always attached.
      { value: primary, label: kilnLabel(primary), hint: 'primary', disabled: true },
    ];
    for (const k of kilns()) {
      if (k.path === primary) continue;
      rows.push({
        value: k.path,
        label: kilnLabel(k.path, k.name),
        hint: connected.has(k.path) ? 'connected' : k.path,
      });
    }
    return rows;
  };

  const kilnTriggerLabel = () => {
    const extra = connectedKilns().length;
    const base = kilnLabel(primaryKiln());
    return extra > 0 ? `${base} +${extra}` : base;
  };

  const toggleKiln = (path: string) => {
    const s = session();
    if (!s || path === s.kiln) return; // primary can't be detached
    if (s.connected_kilns.includes(path)) {
      void mutate(() => disconnectSessionKiln(s.id, path));
    } else {
      void mutate(() => connectSessionKiln(s.id, path));
    }
  };

  return (
    <Show when={session()}>
      <div class="flex items-center gap-1 flex-wrap mt-2" data-testid="context-chips">
        <ChipSelect
          name="project"
          icon={FolderGit2}
          options={projectOptions()}
          value={hasWorkspace() ? session()!.workspace ?? '' : ''}
          triggerLabel={projectLabel()}
          onSelect={pickProject}
          disabled={disabled()}
          testid="scope-project"
        />
        <ChipSelect
          name="kiln"
          icon={FlaskConical}
          multi
          options={kilnOptions()}
          value={primaryKiln()}
          selected={selectedKilns()}
          triggerLabel={kilnTriggerLabel()}
          onSelect={toggleKiln}
          disabled={disabled()}
          testid="scope-kiln"
        />
      </div>
    </Show>
  );
};
