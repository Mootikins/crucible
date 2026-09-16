import { Accessor, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useChatSafe } from '@/contexts/ChatContext';
import { connectSessionKiln, disconnectSessionKiln, listKilns } from '@/lib/api';
import type { KilnListEntry } from '@/lib/types';
import { notificationActions } from '@/stores/notificationStore';
import { pathBasename } from '@/stores/statusBarStore';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
import { swrLocal } from '@/lib/local-cache';
import type { ChipOption } from '@/components/composer/ChipSelect';
import type { ComposerChip } from '@/components/composer/ChipRow';
import { FlaskConical, FolderGit2 } from '@/lib/icons';

/**
 * The session-scope chips for the live composer's row: the workspace the
 * session acts in and the kilns it knows. Same icons as the draft composer
 * (project = FolderGit2, kiln = FlaskConical), and the same row component,
 * so the two surfaces read identically.
 *
 * The project chip is static. A session's workspace is fixed at creation —
 * the daemon refuses `session.set_workspace` for every session — so the chip
 * says which project the session acts in and offers no other. The kiln chip
 * still attaches and detaches mid-session: the daemon rejects mutations
 * mid-turn, re-checks trust on attach, and rebuilds the agent's tools/prompt
 * on the next turn.
 *
 * A hook rather than a component: the row is built from data, and this is
 * where the live session's scope data comes from. Empty with no session.
 */
export function useSessionScopeChips(): Accessor<ComposerChip[]> {
  const { currentSession, applySessionScope } = useSessionSafe();
  const { isStreaming } = useChatSafe();

  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
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
  // No project → the session keeps its own scratch folder.
  const projectLabel = () => {
    const ws = workspace();
    return (ws && pathBasename(ws)) || 'Session folder';
  };

  // ---- kiln chip (one flat multi-select) ----------------------------------
  // No locked primary row: the kiln set is flat, every member detaches the
  // same way, and a session is allowed to reach zero kilns.
  //
  // Every value here is a registry NAME — what `Session.kilns` carries and what
  // `POST /kilns/connect` accepts. The join used to be on `path`, which held
  // only as long as both sides spelled a directory identically; the route now
  // answers 422 to a path, so a path-keyed option would post one and surface
  // the refusal as a toast.
  const selectedKilns = () => session()?.kilns ?? [];

  const kilnOptions = (): ChipOption[] => {
    const attached = new Set(selectedKilns());
    const rows: ChipOption[] = kilns()
      // A listed kiln the daemon could derive no name for cannot be attached —
      // there is nothing to send. Offering the row anyway would post an empty
      // name and show the 422 as a toast, which reads as a broken picker
      // rather than as a kiln that needs registering.
      .filter((k): k is KilnListEntry & { name: string } => !!k.name)
      .map((k) => ({
        value: k.name,
        label: k.name,
        // `kiln.list`'s path is the documented exception to "no paths in the
        // API" — this picker is the surface whose job is to say where a kiln
        // lives, so an unattached row shows the directory it would attach.
        hint: attached.has(k.name) ? 'attached' : k.path,
      }));
    // A member the registry does not answer for still has to be detachable, so
    // it gets a row of its own rather than disappearing from the popout. That
    // covers both an entry deleted from the config and a session file written
    // before names, whose members are paths.
    for (const name of selectedKilns()) {
      if (!kilns().some((k) => k.name === name)) {
        rows.push({ value: name, label: name, hint: 'attached' });
      }
    }
    return rows;
  };

  const kilnTriggerLabel = () => {
    const first = sessionDefaultKiln({ kilns: selectedKilns() });
    // Zero kilns gets its own words: a kiln-less session is a legitimate shape
    // and must not borrow a label from a kiln it has not attached.
    if (first === null) return 'No kiln';
    const extra = selectedKilns().length - 1;
    return extra > 0 ? `${first} +${extra}` : first;
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
  const emptyKilnNote = (
    <div
      class="px-3 py-2 text-xs text-muted-dark border-t border-hairline"
      data-testid="scope-kiln-empty"
    >
      No kiln attached — this session is tools-only. Note search, wikilinks and precognition are
      off until you attach one.
    </div>
  );

  // Multi-select reads `selected`, not `value`, so this is only the anchor for
  // the (unused) single-select path. '' is not a name the registry can issue,
  // which is what keeps a kiln-less session from being labelled by one.
  const kilnChipValue = () => sessionDefaultKiln({ kilns: selectedKilns() }) ?? '';

  const toggleKiln = (name: string) => {
    const s = session();
    if (!s) return;
    if (s.kilns.includes(name)) {
      void mutate(() => disconnectSessionKiln(s.id, name));
    } else {
      void mutate(() => connectSessionKiln(s.id, name));
    }
  };

  return () => {
    if (!session()) return [];
    return [
      {
        key: 'project',
        label: 'Project',
        value: projectLabel(),
        // The title carries the full path, as the kiln rows carry theirs.
        title: workspace() ?? 'Session folder — unique to this session',
        icon: FolderGit2,
        testid: 'scope-project',
        render: 'static',
      },
      {
        key: 'kiln',
        label: 'Kiln',
        value: kilnChipValue(),
        valueLabel: kilnTriggerLabel(),
        icon: FlaskConical,
        options: kilnOptions(),
        onSelect: toggleKiln,
        multi: true,
        selected: selectedKilns(),
        disabled: disabled(),
        testid: 'scope-kiln',
        select: { footer: selectedKilns().length === 0 ? emptyKilnNote : undefined },
      },
    ];
  };
}
