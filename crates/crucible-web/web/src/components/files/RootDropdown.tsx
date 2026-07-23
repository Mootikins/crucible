import { Component, Show, createSignal } from 'solid-js';
import type { RosterGroup, TreeRoot } from '@/lib/tree-root';
import { rosterIndex, rootKey } from '@/lib/tree-root';
import { ChipSelect, type ChipOption } from '@/components/composer/ChipSelect';
import {
  isBranchNameish,
  isGitRepoUrl,
  registerProject,
  scmBranches,
  scmClone,
  scmWorktreeAdd,
  type ScmBranchesResponse,
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
 * project, a Branches section lists every branch of its repo. Picking a
 * branch jumps to its worktree (registering it as a project if needed),
 * offers to CREATE a worktree when the branch has none, and typing an
 * unknown name offers branch-plus-worktree creation.
 */
export const RootDropdown: Component<{
  groups: RosterGroup[];
  selectedKey: string | null;
  onSelect: (r: TreeRoot) => void;
  /** Resolved active root — its repo feeds the Branches section. */
  activeRoot?: TreeRoot | null;
  /** Surface warnings/errors (FilesPanel banner). */
  onNotice?: (msg: string | null) => void;
}> = (props) => {
  const { refreshProjects } = useProjectSafe();
  const index = () => rosterIndex(props.groups);
  const nonEmpty = () => props.groups.filter((g) => g.roots.length > 0);

  const [scm, setScm] = createSignal<ScmBranchesResponse | null>(null);

  // Fetched on every popout open — cheap (one git shell-out) and always fresh.
  const loadBranches = () => {
    const root = props.activeRoot;
    if (!root || root.kind !== 'project') {
      setScm(null);
      return;
    }
    // Out-of-order guard: only apply the response if the active root hasn't
    // changed since the fetch started.
    const forPath = root.path;
    scmBranches(forPath)
      .then((res) => props.activeRoot?.path === forPath && setScm(res))
      .catch(() => props.activeRoot?.path === forPath && setScm(null)); // repo-less: no section
  };

  const options = (): ChipOption[] => {
    const rosterOptions = nonEmpty().flatMap((g) =>
      g.roots.map((r) => ({
        value: rootKey(r),
        label: r.name,
        group: g.label as string,
      })),
    );
    const s = scm();
    const branchOptions = (s?.branches ?? []).map((b) => ({
      value: `branch:${b.name}`,
      label: b.name,
      group: `Branches — ${basename(s!.repo_root)}`,
      hint: b.is_current
        ? 'current'
        : b.worktree_path
          ? basename(b.worktree_path)
          : b.remote_only
            ? 'remote · create worktree'
            : 'create worktree',
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

  const createWorktree = async (repoRoot: string, branch: string, createBranch: boolean) => {
    try {
      const res = await scmWorktreeAdd(repoRoot, branch, createBranch);
      props.onNotice?.(res.warning ?? null);
      await refreshProjects();
      props.onSelect({ kind: 'project', path: res.path, name: basename(res.path) });
    } catch (e) {
      props.onNotice?.(e instanceof Error ? e.message : 'Failed to create worktree');
    }
  };

  const pickBranch = (name: string) => {
    const s = scm();
    const branch = s?.branches.find((b) => b.name === name);
    if (!s || !branch) return;
    if (branch.worktree_path) {
      void selectWorktreeRoot(branch.worktree_path);
      return;
    }
    if (window.confirm(`No worktree for '${name}' — create one?`)) {
      void createWorktree(s.repo_root, name, false);
    }
  };

  const onPick = (value: string) => {
    if (value.startsWith('branch:')) {
      pickBranch(value.slice('branch:'.length));
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
        value={props.selectedKey ?? ''}
        onSelect={onPick}
        onOpen={loadBranches}
        testid="root-dropdown"
        triggerClass="inline-flex items-center gap-1 max-w-[12rem] bg-surface-elevated text-shell-ink text-xs px-2 py-1 rounded border border-hairline hover:border-hairline-strong transition-colors"
        action={{
          label: 'Clone a repository…',
          placeholder: 'github.com/owner/repo or git URL',
          buttonLabel: 'Clone',
          validate: isGitRepoUrl,
          run: (url) => void cloneRepo(url),
        }}
        create={
          scm()
            ? {
                // Branch names only — explicit URL forms (https://, git@…)
                // contain ':' so isBranchNameish already excludes them; the
                // clone action row owns those. owner/repo-shaped text stays
                // valid as a branch name (feature/x is the common case).
                when: isBranchNameish,
                label: (text) => `Create branch + worktree '${text}'`,
                run: (text) => {
                  const s = scm();
                  if (!s) return;
                  if (window.confirm(`Create branch '${text}' and a worktree for it?`)) {
                    void createWorktree(s.repo_root, text, true);
                  }
                },
              }
            : undefined
        }
      />
    </Show>
  );
};
