import { Component } from 'solid-js';
import type { RosterGroup, TreeRoot } from '@/lib/tree-root';
import { rosterIndex, rootKey } from '@/lib/tree-root';
import type { SessionRoot } from '@/lib/session-roots';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import { isGitRepoUrl } from '@/lib/api';
import { useRegisterProject, useScmClone } from '@/lib/query/projects';
import { useResolveWorkspaceTarget, useWorkspaceTargets } from '@/lib/query/targets';

function basename(p: string): string {
  const parts = p.replace(/\/$/, '').split('/');
  return parts[parts.length - 1] || p;
}

/** Section header for the session's own roots — its workspace and its kilns. */
const OWN_GROUP = 'This session';

/**
 * The file tree's ONLY root control: one dropdown that selects one root.
 *
 * It is a selector, not a tab bar. An earlier version paired this popout with
 * a strip of tabs for the session's own roots, which gave the panel two
 * controls for one piece of state — the tabs and the menu disagreed about what
 * "selected" meant, and a root picked from the menu had no tab to live in. The
 * session's roots are now the first section of this list, so there is one
 * control, one selection and one label.
 *
 * The trigger is ALWAYS rendered, including with an empty roster. It reads "No
 * roots" and still opens: the Clone action is how a fresh install gets its
 * first root, so hiding the control makes the empty state a dead end.
 *
 * It uses the composer's ChipSelect idiom (searchable list, grouped sections)
 * instead of a native `<select>`, because the roster goes beyond existing
 * roots: when the active root is a git project, a Branches section lists every
 * workspace target its providers offer. Picking one jumps to its checkout,
 * creating it if there is none.
 *
 * The branch list and the creation both come from the workspace provider that
 * owns them. This file used to call `scm.branches` and `scm.worktree_add`
 * directly, which put git in the rendering layer and gave the daemon a second
 * copy of what a branch is.
 */
export const RootDropdown: Component<{
  /** The session's own roots — workspace and attached kilns. They lead the list. */
  own: SessionRoot[];
  /** Full registry roster: every project, worktree and kiln. */
  groups: RosterGroup[];
  selectedKey: string | null;
  onSelect: (r: TreeRoot) => void;
  /** Resolved active root — its repo feeds the Branches section and its name
   * labels the trigger (the dropdown IS the current-root display). */
  activeRoot?: TreeRoot | null;
  /** Surface warnings/errors (FilesPanel banner). */
  onNotice?: (msg: string | null) => void;
}> = (props) => {
  const register = useRegisterProject();
  const clone = useScmClone();
  const ownKeys = () => new Set(props.own.map(rootKey));
  // Own roots override their roster twins, so one key resolves to one row.
  const index = () => {
    const idx = rosterIndex(props.groups);
    for (const r of props.own) idx.set(rootKey(r), r);
    return idx;
  };
  const hasRoots = () => props.own.length > 0 || props.groups.some((g) => g.roots.length > 0);

  const resolve = useResolveWorkspaceTarget();

  /** The repository the Branches section lists, or nothing to list. */
  const repoPath = () =>
    props.activeRoot?.kind === 'project' ? props.activeRoot.path : undefined;
  // Keyed by that repository, which is what replaces the out-of-order guard
  // this list used to need: an answer for the root the user left lands in that
  // root's entry, not in the menu. The sessions rail reads the same key, so
  // opening this popout over a repo the rail already labelled asks nothing.
  const targetsQuery = useWorkspaceTargets(repoPath);
  const targets = () => targetsQuery.data ?? [];

  // Still asked on every popout open — one git shell-out, and a branch list
  // the user is about to read has to be current.
  const loadBranches = () => {
    if (repoPath() !== undefined) void targetsQuery.refetch();
  };

  const options = (): ChipOption[] => {
    const ownOptions = props.own.map((r) => ({
      value: rootKey(r),
      label: r.name,
      group: OWN_GROUP,
      hint: r.origin === 'workspace' ? 'workspace' : 'attached',
    }));
    // A root the session does not own is browsable but NOT readable by the
    // agent. Say so on the row: picking one is navigation, and a navigation
    // gesture must never read as widening what the agent can see.
    const skip = ownKeys();
    const rosterOptions = props.groups.flatMap((g) =>
      g.roots
        .filter((r) => !skip.has(rootKey(r)))
        .map((r) => ({
          value: rootKey(r),
          label: r.name,
          group: g.label as string,
          hint: 'browse only',
        })),
    );
    const repo = props.activeRoot?.path;
    const branchOptions = targets().map((t) => ({
      value: `target:${t.spec}`,
      label: t.label,
      group: repo ? `Branches — ${basename(repo)}` : 'Branches',
      hint: t.hint,
    }));
    return [...ownOptions, ...rosterOptions, ...branchOptions];
  };

  const selectWorktreeRoot = async (path: string) => {
    // The worktree may exist on disk without being a registered project —
    // register (idempotent) so the roster lists it, then select.
    // The mutation refreshes the roster before it settles, so the row exists
    // by the time the tree is re-rooted on it.
    try {
      await register.mutateAsync(path);
    } catch {
      /* already registered */
    }
    props.onSelect({ kind: 'project', path, name: basename(path) });
  };

  const cloneRepo = async (url: string) => {
    try {
      const res = await clone.mutateAsync(url);
      props.onNotice?.(null);
      props.onSelect({ kind: 'project', path: res.path, name: basename(res.path) });
    } catch (e) {
      props.onNotice?.(e instanceof Error ? e.message : 'Failed to clone repository');
    }
  };

  /**
   * Jump to a target's checkout, asking its provider to materialise one when
   * it has none.
   *
   * No confirmation prompt. Picking a branch from a list headed "create
   * worktree" IS the confirmation, and the provider is idempotent — asking
   * twice for the same branch returns the same checkout rather than failing.
   */
  const pickTarget = (spec: string) => {
    const known = targets().find((t) => t.spec === spec);
    if (known?.path) {
      void selectWorktreeRoot(known.path);
      return;
    }
    void (async () => {
      try {
        const path = await resolve.mutateAsync({ spec, workspace: props.activeRoot?.path });
        props.onNotice?.(null);
        await selectWorktreeRoot(path);
      } catch (e) {
        props.onNotice?.(e instanceof Error ? e.message : 'Failed to resolve workspace target');
      }
    })();
  };

  const onPick = (value: string) => {
    if (value.startsWith('target:')) {
      pickTarget(value.slice('target:'.length));
      return;
    }
    const r = index().get(value);
    if (r) props.onSelect(r);
  };

  return (
    <ChipSelect
      name="Browse root"
      options={options()}
      // The trigger IS the current-root display: picking from the menu re-roots
      // the tree in place, so the label must always name the resolved active
      // root — including one that is not itself a roster row (an unregistered
      // workspace). With nothing to browse it says so and still opens, because
      // the Clone action inside is the way out of an empty roster.
      triggerLabel={props.activeRoot?.name ?? (hasRoots() ? undefined : 'No roots')}
      value={props.selectedKey ?? ''}
      onSelect={onPick}
      onOpen={loadBranches}
      testid="root-dropdown"
      triggerClass="inline-flex items-center gap-1 min-w-0 max-w-[12rem] h-7 bg-surface-elevated text-shell-ink text-xs px-2 rounded border border-hairline hover:border-hairline-strong transition-colors"
      action={{
        label: 'Clone a repository…',
        placeholder: 'github.com/owner/repo or git URL',
        buttonLabel: 'Clone',
        validate: isGitRepoUrl,
        run: (url) => void cloneRepo(url),
      }}
      create={
        targets().length > 0
          ? {
              // Branch names only — explicit URL forms (https://, git@…)
              // contain ':' and are excluded here; the clone action row owns
              // those. owner/repo-shaped text stays valid as a branch name
              // (feature/x is the common case).
              when: (text) => !!text && !/\s|\.\.|^[-/]|\\|:|@\{|\/$/.test(text),
              label: (text) => `Create branch + worktree '${text}'`,
              // The provider validates the name properly and refuses what it
              // cannot honour; the guard above only keeps clone URLs out of
              // this row.
              run: (text) => pickTarget(`${targets()[0].spec.split(':')[0]}:${text}`),
            }
          : undefined
      }
    />
  );
};
