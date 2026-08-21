import { Component, Show, createSignal } from 'solid-js';
import type { RosterGroup, TreeRoot } from '@/lib/tree-root';
import { rosterIndex, rootKey } from '@/lib/tree-root';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import {
  isGitRepoUrl,
  listWorkspaceTargets,
  registerProject,
  resolveWorkspaceTarget,
  scmClone,
  type ProviderTarget,
} from '@/lib/api';
import { useProjectSafe } from '@/contexts/ProjectContext';

function basename(p: string): string {
  const parts = p.replace(/\/$/, '').split('/');
  return parts[parts.length - 1] || p;
}

/**
 * Popout picker for the browsable root — the composer's ChipSelect idiom
 * (searchable list, grouped sections) instead of a native `<select>`, because
 * the roster now goes beyond existing roots: when the active root is a git
 * project, a Branches section lists every workspace target its providers
 * offer. Picking one jumps to its checkout, creating it if there is none.
 *
 * The branch list and the creation both come from the workspace provider that
 * owns them. This file used to call `scm.branches` and `scm.worktree_add`
 * directly, which put git in the rendering layer and gave the daemon a second
 * copy of what a branch is.
 */
export const RootDropdown: Component<{
  groups: RosterGroup[];
  selectedKey: string | null;
  onSelect: (r: TreeRoot) => void;
  /** Resolved active root — its repo feeds the Branches section. */
  activeRoot?: TreeRoot | null;
  /** Surface warnings/errors (FilesPanel banner). */
  onNotice?: (msg: string | null) => void;
  /**
   * Render as a bare chevron: the overflow at the end of `RootStrip`, where
   * the strip already names the active root and a second label would repeat
   * it. Suppresses the trigger's text, not its menu.
   */
  bare?: boolean;
}> = (props) => {
  const { refreshProjects } = useProjectSafe();
  const index = () => rosterIndex(props.groups);
  const nonEmpty = () => props.groups.filter((g) => g.roots.length > 0);

  const [targets, setTargets] = createSignal<ProviderTarget[]>([]);

  // Fetched on every popout open — cheap (one git shell-out) and always fresh.
  const loadBranches = () => {
    const root = props.activeRoot;
    if (!root || root.kind !== 'project') {
      setTargets([]);
      return;
    }
    // Out-of-order guard: only apply the answer if the active root hasn't
    // changed since the fetch started.
    const forPath = root.path;
    void listWorkspaceTargets(forPath).then(
      (rows) => props.activeRoot?.path === forPath && setTargets(rows),
    );
  };

  const options = (): ChipOption[] => {
    const rosterOptions = nonEmpty().flatMap((g) =>
      g.roots.map((r) => ({
        value: rootKey(r),
        label: r.name,
        group: g.label as string,
      })),
    );
    const repo = props.activeRoot?.path;
    const branchOptions = targets().map((t) => ({
      value: `target:${t.spec}`,
      label: t.label,
      group: repo ? `Branches — ${basename(repo)}` : 'Branches',
      hint: t.hint,
    }));
    return [...rosterOptions, ...branchOptions];
  };

  const selectWorktreeRoot = async (path: string) => {
    // The worktree may exist on disk without being a registered project —
    // register (idempotent) so the roster lists it, then select.
    try {
      await registerProject(path);
    } catch {
      /* already registered */
    }
    await refreshProjects();
    props.onSelect({ kind: 'project', path, name: basename(path) });
  };

  const cloneRepo = async (url: string) => {
    try {
      const res = await scmClone(url);
      props.onNotice?.(null);
      await refreshProjects();
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
        const path = await resolveWorkspaceTarget(spec, props.activeRoot?.path);
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
    <Show
      when={nonEmpty().length > 0}
      fallback={<span class="text-xs text-muted-dark">No roots</span>}
    >
      <ChipSelect
        name="Browse root"
        options={options()}
        // Bare: an empty placeholder leaves the trigger's label empty, so the
        // chevron stands alone beside the strip that already names the root.
        placeholder={props.bare ? '' : undefined}
        value={props.bare ? '' : (props.selectedKey ?? '')}
        onSelect={onPick}
        onOpen={loadBranches}
        testid="root-dropdown"
        triggerClass={
          props.bare
            ? 'inline-flex items-center px-1 py-1 rounded text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors'
            : 'inline-flex items-center gap-1 max-w-[12rem] bg-surface-elevated text-shell-ink text-xs px-2 py-1 rounded border border-hairline hover:border-hairline-strong transition-colors'
        }
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
    </Show>
  );
};
