import { Accessor, createSignal } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useChatSafe } from '@/contexts/ChatContext';
import type { SessionScope } from '@/lib/api';
import { useConnectSessionKiln, useDisconnectSessionKiln } from '@/lib/query/scope';
import { notificationActions } from '@/stores/notificationStore';
import { pathBasename } from '@/stores/statusBarStore';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
import { attachableKilns } from '@/lib/kiln-registry';
import { useKilns } from '@/lib/query/kilns';
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

  // The shell's one roster; its last-known list paints the chips on reload.
  const kilnsQuery = useKilns();
  const kilns = () => kilnsQuery.data ?? [];
  const [busy, setBusy] = createSignal(false);

  // The two writes, which fold the scope the daemon echoes into every cached
  // copy of the session. The files panel still attaches a kiln through
  // `applySessionScope`, so the context keeps that entry point.
  const connect = useConnectSessionKiln();
  const disconnect = useDisconnectSessionKiln();

  const session = () => currentSession();
  const workspace = () => {
    const s = session();
    return s ? sessionWorkspace(s) : null;
  };
  const disabled = () => busy() || isStreaming();

  const mutate = async (action: () => Promise<SessionScope>) => {
    if (disabled()) return;
    setBusy(true);
    try {
      // The mutation patches the cache; this hands the same echo to the
      // context, whose selection signal these chips read.
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
  //
  // An ephemeral session HAS a workspace: the daemon gives it
  // `<session_scratch_dir>/<id>`, whose basename is the session id. Showing
  // that made the widest chip on the row a 36-character identifier naming a
  // directory nobody types — it pushed every other chip off a narrow pane and
  // said nothing when it fitted. So a folder the session owns reads as one,
  // and the title still carries the full path, id included.
  const projectLabel = () => {
    const ws = workspace();
    if (!ws) return 'Session folder';
    const base = pathBasename(ws);
    const id = session()?.session_id;
    if (!base || (id && base === id)) return 'Session folder';
    return base;
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
    // A row the daemon cannot resolve is not offered. It says so itself, with
    // `registered: false`, and the rule is spelled once in `attachableKilns`
    // so the draft composer and this one cannot disagree about it.
    const rows: ChipOption[] = attachableKilns(kilns())
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
      void mutate(() => disconnect.mutateAsync({ id: s.session_id, kiln: name }));
    } else {
      void mutate(() => connect.mutateAsync({ id: s.session_id, kiln: name }));
    }
  };

  return () => {
    if (!session()) return [];
    return [
      {
        key: 'project',
        // Where the session acts outranks what it knows: a session in the
        // wrong folder is a different mistake from one missing a kiln. Both
        // sit after the model and the mode, which the live composer adds.
        priority: 30,
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
        priority: 40,
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
