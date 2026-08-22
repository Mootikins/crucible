---
title: Workspace and Runtime Targets
description: Two orthogonal axes. One axis says where the files of a session live. The other says where its process runs. Plugins contribute both.
status: design
tags:
  - architecture
  - plugins
  - isolation
  - web
aliases:
  - Run On
  - Target Axes
---

# Workspace and Runtime Targets

As-built detail: [[Actual]]

This note records the design and its rationale. [[Actual]] lists the types and the
files. Steps 1, 2 and 4 of the sequence are done. The ssh plugin (step 3) does not
exist.

See [[Container Isolation]] for the plugin this design generalises. See
[[Plugin Conventions]] for the contribution channels it uses.

## The two axes

Do not treat "worktree", "container" and "remote machine" as three values of one
setting. They are values of **two** settings. The combinations that cross the two
settings are the interesting ones.

| Axis | Question | Providers | Mechanism |
|------|----------|-----------|-----------|
| **Workspace** | Where do the files live? | `worktree` (later: clone, remote folder) | The provider rewrites the workspace path before the daemon creates the session |
| **Runtime** | Where does the process run? | `oci` (later: `ssh`) | `crucible.require_isolation` + `SandboxExec` |

```
  main    × host             ordinary session
  main    × container:rust   oci
  feat/x  × host             the parallel-agents flow
  feat/x  × container:rust   worktree × oci
  remote  × ssh:build-box    both axes collapse (files and process are remote)
```

The oci plugin runs one container per distinct workspace, not per session. A session
with its own worktree has a distinct workspace. It therefore gets its own container,
with no branch added anywhere.

The split also removes a collision. `session.isolation` is one opaque value that each
isolating plugin reads. A `worktree` value sent down that channel would reach `oci`.
Under the two-axis model the worktree plugin never touches `session.isolation`. For the
runtime axis, where two plugins share one channel, a target names its provider (below).

`ssh` sits across both axes, because remote files and a remote process are the same
fact. It is a runtime provider that names a remote directory. The workspace axis then
resolves in that context. See [What an ssh target means](#what-an-ssh-target-means).

**Container-on-remote (`ssh host podman exec …`) is out of scope.** One runtime
provider wins. A list of prefixes could express it later; see the end of this note.

## How a plugin contributes a target

A plugin needs two things. Both channels already exist.

**Declare the provider** with `crucible.publish`:

```lua
crucible.publish("targets", {
  axis            = "workspace",        -- or "runtime"
  label           = "Worktree",
  targets_command = "worktree.targets", -- enumerated on demand
  resolve_command = "worktree.resolve", -- workspace axis only
})
```

The worktree plugin does this at `runtime/plugins/worktree/init.lua:190`. The oci
plugin does it at `runtime/plugins/oci/init.lua:848`.

**Enumerate targets** with an ordinary plugin command. The web calls it through the
`plugin.run_command { name, args } → { result }` RPC (`web/src/lib/api.ts:547`).

Enumeration is a command, not published data, because the workspace axis is dynamic.
The branch list depends on the selected project. It changes when a user creates a
branch outside the app. The `oci` profile list is static, but one uniform shape is
better than two.

### Addressing a runtime target

`oci` and a future `ssh` share the `session.isolation` channel. A target therefore
names its provider:

```
session.create { isolation = { plugin = "ssh", target = "build-box" } }
```

A runtime plugin ignores a table that names a different plugin. It returns `nil`
(`oci/init.lua:399`). It does not raise. A bare unknown profile name is still an
error (`oci/init.lua:386`). Bare `true` / `false` / `"profile-name"` continue to work
(`oci/init.lua:381-389`), because every existing config sends those.

## Where the daemon creates a worktree

The workspace axis must change the workspace path before the session exists. The
`on_session_start` hook fires too late:

```
session.create
  ├─ resolve_workspace_target          dispatch.rs:1265  ← NEW, before all three
  ├─ resolve_create_agent(workspace)   create.rs:204     ← the agent cwd is fixed here
  ├─ pm.register_if_missing(workspace) create.rs:247     ← the project is registered here
  ├─ sm.create_session(workspace)      create.rs:254     ← the workspace is persisted here
  └─ enforce_session_start             session_lifecycle.rs:87 ← on_session_start fires here
```

A plugin that rewrote the workspace in `on_session_start` would leave the project
registered at the old path. An ACP agent would already point at the old path.

So resolution runs first, in `DaemonDispatch::resolve_workspace_target`
(`rpc/dispatch.rs:1293`). It is a resolution step, not a new hook list. The provider
names a `resolve_command` beside its `targets_command`. The daemon calls it through
`plugin.run_command`. A second hook registry would gain nothing: `crucible.on(...)`
broadcasts to every listener, but exactly one provider must answer a target that names
it.

Resolution is fail-closed at every step: unknown provider, wrong axis, missing command,
command error, relative or empty path. An unresolved target returns `INVALID_PARAMS`
(`dispatch.rs:1318`). `path_from_result` rejects an empty or relative path
(`workspace_targets.rs:54`). A session that asked for `feat/x` never falls back to
`main`, because an agent that works on `main` commits there.

**Delegation does not go through this step.** `delegation.rs` calls
`SessionManager::create_child_session` (`delegation.rs:471`). The child inherits the
workspace of its parent (`delegation.rs:385`, `:434`). `workspace_target` does not
appear in `delegation.rs`. A subagent cannot get its own worktree today. To add it,
route the child create through the same resolution step.

## The worktree plugin

`runtime/plugins/worktree/` shells out to `git` (`init.lua:38`, `:85`), as `oci` shells
out to `podman`. Rust holds no git knowledge for worktrees.

```lua
cru.shell.exec("git", { "-C", repo, "worktree", "add", dest, branch })
```

The plugin replaced `scm.branches`, `scm.worktree_add`, `/api/scm/branches`,
`/api/scm/worktree`, `collect_branches` and `add_worktree`. The branch-name validation,
the destination template, the porcelain parser and the branch sort moved to Lua with
their tests. `scm.clone` stays in Rust (`dispatch.rs:212`), because a clone is a
separate concern with no plugin behind it.

## The ssh plugin (not implemented)

No `runtime/plugins/ssh/` exists. This section is the proposal.

`SandboxExec` nearly fits:

```lua
crucible.require_isolation{
  session     = session.id,
  plugin      = "ssh",
  exec_prefix = { "ssh", "-T", host },
  exec_suffix = {},
}
```

One gap is closed. `exec_env_flag` assumed a launcher that repeats a flag per variable
(`-e K=V`), which is how `podman exec` works. `ssh` has no such flag. Its idiom is a
positional `env K=V … cmd`. `SandboxExec.env` is now the three-state `SandboxEnv`:
`Unsupported` (the default), `Flag(String)`, `Inline`
(`crucible-lua/src/isolation.rs:94`). Lua declares it as `exec_env_flag = "-e"` or
`exec_env_inline = true` (`isolation.rs:230-240`).

### What an ssh target means

**A remote directory.** `ssh:build-box` names a machine. The workspace is a path on
that machine. There is no sync, no mount and no mirror. Only the daemon is local.

Two consequences follow:

`build_client_config` sets `working_dir` on the **launcher** process
(`acp_launch.rs:69`). For `podman exec` this does not matter, because the prefix
carries `-w`. For ssh it would set the directory of the local client. The remote agent
would start in its login home. The prefix must carry the remote directory:
`ssh -T build-box cd <dir> && env … agent`.

A locally resolved workspace path has no meaning on the remote host.
`worktree.resolve` runs daemon-side and answers with a local path. The same string on
`build-box` may not exist.

### ssh × worktree: provision remotely

The daemon performs the combination on the far side. For `worktree:feat/x` on
`ssh:build-box`, the ssh provider:

1. clones the repo on the remote host under a configured base directory, if it is not
   there;
2. creates the worktree there, by the rules the local worktree provider uses;
3. answers with the **remote** path. The runtime prefix then enters that path.

So the workspace axis must know that the runtime is remote. The axes stay orthogonal
in meaning, but resolution is ordered: runtime first, then workspace in its context.
That order is the one real coupling.

The daemon does not guard this combination. It is unreachable until an ssh provider
exists. A guard written now would guess the shape of the thing it guards.

### Containers on top, in theory

A container is a post-resolution step on the host where the workspace ended up. In
prefix terms that is composition: `ssh -T build-box`, then
`podman exec -i -w <remote dir>`. Each provider contributes its own segment.
`SandboxExec` already concatenates prefix, env and suffix.

What must change: the runtime axis takes exactly one provider, and `session.isolation`
carries one target. A list is the whole structural difference. We do not do it. The
reason is scope, not a wall.

## Prior art

A survey of Claude Code, Cursor 3.x, Zed/ACP, VS Code agent plugins, OpenCode,
Continue.dev, Aider and Pi found **no system that models workspace location and
process location as separate axes**. None lets a plugin enumerate run targets.

| Tool | Workspace axis | Runtime axis | Who supplies the targets |
|------|---------------|--------------|--------------------------|
| Claude Code | worktrees (`--worktree`) | none, always local | CLI flag, static `.worktreeinclude` |
| Cursor 3.x | worktrees | cloud / SSH / local | built-in UI; static `.cursor/worktrees.json` |
| Zed (ACP) | none | none | — |
| VS Code, Continue, Aider, OpenCode | none | implicit local | static manifests or config files |

Cursor is closest. It treats the two as correlated pairs, chosen per session from a
built-in picker.

Two consequences: First, there is nobody to copy. Second, keep the axes cheap to
collapse. No one has asked for `worktree × ssh` yet. `worktree × container` is the
only crossing with a real user.

## Web

`ChipSelect` has `children?: ChipOption[]` (`components/composer/ChipSelect.tsx:31`)
and a drill-down. The chip row is: project · workspace target · runtime target · agent
· model. Both target chips come from `getTargetProviders` (`web/src/lib/api.ts:555`).
The `run on` chip has a built-in `This PC` row (`components/CenterComposer.tsx:352`).

The old `run on` chip and the isolation chip asked one question twice. They are now
one chip. One provider on an axis flattens its targets into the menu. Two or more get a
`▸` drill-down.

## Sequencing

1. **Groundwork** — done. `workspace_targets` resolution before create, the `targets`
   channel, `plugin.run_command` from the web, `ChipSelect` submenus, `SandboxEnv`.
2. **Worktree plugin and the composer** — done.
3. **SSH plugin** — not done.
4. **Retire the `scm.*` worktree RPCs** — done, in the same pass as step 2.

## Known limits

- One runtime provider per session. Container-on-remote is not expressible.
- For a remote runtime, the workspace axis must resolve in the context of the runtime.
  Unreachable until an ssh provider exists.
- The workspace axis rewrites a path. It does not sync, copy or clean up. A worktree
  outlives its session.
- `session.isolation` keeps its untyped shape. A malformed target is a plugin-side
  error, not a schema rejection.
- Delegation does not resolve `workspace_target`. A child session inherits the
  workspace of its parent.
