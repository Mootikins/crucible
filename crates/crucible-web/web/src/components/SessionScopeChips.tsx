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
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
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
  const workspace = () => {
    const s = session();
    return s ? sessionWorkspace(s) : null;
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
    const ws = workspace();
    return (ws && pathBasename(ws)) || 'Session folder';
  };

  // ---- kiln chip (one flat multi-select) ----------------------------------
  // No locked primary row: the kiln set is flat, every member detaches the
  // same way, and a session is allowed to reach zero kilns.
  const selectedKilns = () => session()?.kilns ?? [];

  const kilnOptions = (): ChipOption[] => {
    const attached = new Set(selectedKilns());
    const rows: ChipOption[] = kilns().map((k) => ({
      value: k.path,
      label: kilnLabel(k.path, k.name),
      hint: attached.has(k.path) ? 'attached' : k.path,
    }));
    // An attached kiln missing from the registry still has to be detachable,
    // so it gets a row of its own rather than disappearing from the popout.
    for (const path of selectedKilns()) {
      if (!kilns().some((k) => k.path === path)) {
        rows.push({ value: path, label: kilnLabel(path), hint: 'attached' });
      }
    }
    return rows;
  };

  const kilnTriggerLabel = () => {
    const first = sessionDefaultKiln({ kilns: selectedKilns() });
    // Not `kilnLabel('')` — an empty path is the home data dir to that helper,
    // so a kiln-less session would advertise itself as attached to the home
    // kiln. Zero kilns gets its own words.
    if (first === null) return 'No kiln';
    const extra = selectedKilns().length - 1;
    const base = kilnLabel(first, kilns().find((k) => k.path === first)?.name);
    return extra > 0 ? `${base} +${extra}` : base;
  };

  /**
   * What the popout says when nothing is attached.
   *
   * Zero kilns is a legitimate session shape — a tools-only agent — so this is
   * a description, not an error: no red, no "fix this". It does have to be
   * explicit about what is gone, because the chip alone reading "No kiln"
   * leaves it plausible that note search still works. It does not: the daemon
   * does not register the knowledge tools for a kiln-less session.
   */
  const emptyKilnNote = () => (
    <div
      class="px-3 py-2 text-xs text-muted-dark border-t border-hairline"
      data-testid="scope-kiln-empty"
    >
      No kiln attached — this session is tools-only. Note search, wikilinks and precognition are
      off until you attach one.
    </div>
  );

  // Multi-select reads `selected`, not `value`, so this is only the anchor for
  // the (unused) single-select path. '' never resolves to a kiln, which is what
  // keeps a kiln-less session from being labelled by one.
  const kilnChipValue = () => sessionDefaultKiln({ kilns: selectedKilns() }) ?? '';

  const toggleKiln = (path: string) => {
    const s = session();
    if (!s) return;
    if (s.kilns.includes(path)) {
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
          value={workspace() ?? ''}
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
          value={kilnChipValue()}
          selected={selectedKilns()}
          triggerLabel={kilnTriggerLabel()}
          onSelect={toggleKiln}
          disabled={disabled()}
          testid="scope-kiln"
          footer={selectedKilns().length === 0 ? emptyKilnNote() : undefined}
        />
      </div>
    </Show>
  );
};
