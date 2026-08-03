---
title: Workspace and Runtime Targets
description: Two orthogonal axes — where a session's files live, and where its process runs — both contributed by plugins
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

The `oci` plugin containerises a session. Nothing else can. Worktrees are hardcoded
Rust plus a hundred lines of orchestration in the web composer, and running a session
on another machine is not expressible at all. This is the design that turns all three
into the same kind of thing: a *target* a plugin contributes.

See [[Container Isolation]] for the plugin this generalises from, and
[[Plugin Conventions]] for the contribution channels it builds on.

## The two axes

The mistake to avoid is treating "worktree", "container" and "remote machine" as three
values of one setting. They are values of **two** settings, and the interesting
combinations are the ones that cross:

| Axis | Question | Providers | Mechanism |
|------|----------|-----------|-----------|
| **Workspace** | where do the files live? | `worktree` (later: clone, remote folder) | rewrites the session's workspace path before creation |
| **Runtime** | where does the process run? | `oci`, `ssh` | `crucible.require_isolation` + `SandboxExec` |

```
crucible ⌄   ⎇ feat/x · new worktree ⌄   ▣ Container · rust ⌄
             └─ WORKSPACE axis           └─ RUNTIME axis

  main    × host             ordinary session
  main    × container:rust   today's oci
  feat/x  × host             today's parallel-agents flow
  feat/x  × container:rust   ← already assumed to work, nothing declares it
  remote  × ssh:build-box    collapses both (files and process are remote)
```

That fourth row is not speculative. `runtime/plugins/oci/init.lua` already reasons about
it:

> One container per distinct WORKSPACE, not per session. […] When a child gets its own
> worktree it has a distinct workspace and therefore its own container, by the same rule
> and with no branch added anywhere.

The oci plugin was written expecting worktrees to exist as a separate axis. They just
never did.

The split also dissolves a collision that would otherwise appear the moment a second
isolating plugin ships. `session.isolation` is one opaque value that every isolating
plugin reads and interprets independently, and `oci`'s resolver **raises** on a name it
does not recognise:

```lua
local profile = config.profiles and config.profiles[requested]
if not profile then
  error("oci: unknown isolation profile '" .. requested .. "'")
end
```

A `worktree` target sent down that channel is a hard error in an unrelated plugin. Under
the two-axis model the worktree plugin never touches `session.isolation` at all, so the
question does not arise. For the runtime axis — where `oci` and `ssh` genuinely do share
a channel — targets are addressed to their provider (below).

`ssh` sits across both axes, because files being remote and the process being remote are
the same fact. It is a runtime provider naming a remote directory, and the workspace axis
then resolves *in that context* — see [What an ssh target means](#what-an-ssh-target-means).
Cursor collapses them the same way: its folder picker groups repositories under
`On This PC` / `Cloud` / `<machine>`.

**Container-on-remote (`ssh host podman exec …`) is out of scope** — one runtime provider
wins. The shape would express it as a list of prefixes rather than one; that is scope, not
a wall, and the sketch is at the end.

## How a plugin contributes a target

Two things are needed, and both already exist as channels.

**Declaring the provider** — `crucible.publish`, the generic contribution channel:

```lua
crucible.publish("targets", {
  axis            = "workspace",        -- or "runtime"
  label           = "Worktree",
  targets_command = "worktree.targets", -- enumerated on demand
  resolve_command = "worktree.resolve", -- workspace axis only
})
```

**Enumerating targets** — an ordinary plugin command, invoked through the existing
`plugin.run_command { name, args } → { result }` RPC:

```lua
crucible.command("worktree.targets", function(args)
  -- args.workspace is the currently selected project
  return { { value = "feat/x", label = "feat/x", hint = "new worktree" }, … }
end)
```

Enumeration has to be a command rather than more published data because the workspace
axis is **dynamic and context-dependent**: the branch list depends on which project is
selected, and changes when someone creates a branch outside the app. `oci`'s profile list
is static and could have been published directly, but one uniform shape beats two.

This replaces `getIsolationOffer`'s bespoke `{ available, profiles: string[] }` merge with
something a plugin shipped tomorrow can use unchanged.

### Addressing a runtime target

`oci` and `ssh` share the `session.isolation` channel, so a target names its provider:

```
session.create { isolation = { plugin = "ssh", target = "build-box" } }
```

A runtime plugin ignores any table addressed to a different plugin, rather than raising.
Bare `true` / `false` / `"profile-name"` keep working exactly as they do now — `oci` is
the only runtime provider today and every existing config sends those.

## Where a worktree actually gets created

The workspace axis needs the session's workspace path to *change* before the session
exists. Today the only plugin entry point is `on_session_start`, which fires far too late:

```
session.create
  ├─ resolve_create_agent(workspace)     create.rs:96   ← agent's cwd fixed here
  ├─ pm.register_if_missing(workspace)   create.rs:109  ← project registered here
  ├─ sm.create_session(workspace)        create.rs:115  ← workspace persisted here
  └─ returns
       └─ enforce_session_start          dispatch.rs:838 ← on_session_start fires HERE
```

A plugin that rewrote the workspace in `on_session_start` would leave the project
registered at the old path and an ACP agent already pointed at it.

So resolution happens **before** all three, in the dispatch wrapper that already owns the
create call:

```
session.create { workspace = '/repo', workspace_target = 'worktree:feat/x' }
  ├─ NEW: resolve_workspace_target        → '/repo/tree/feat/x'
  │        └─ provider's resolve_command, via plugin.run_command
  ├─ resolve_create_agent(workspace)     ← sees the new path
  ├─ pm.register_if_missing(workspace)   ← registers the new path
  └─ sm.create_session(workspace)
```

It is a resolution *step*, not a new hook list. The provider names a `resolve_command`
beside its `targets_command`, and the daemon invokes it through the `plugin.run_command`
machinery that already exists — the same channel enumeration uses. A second hook registry
would have bought nothing: `crucible.on(...)` broadcasts to every listener, whereas
exactly one provider must answer a target addressed to it by name.

Doing it daemon-side rather than having the client resolve-then-create is what makes
**server-side delegation work for free**: `delegation.rs` creates child sessions with no
client involved, so a subagent can be given its own worktree by passing a
`workspace_target`, with nothing added to the delegation path.

Resolution is fail-closed at every step — unknown provider, wrong axis, missing command,
command error, relative or empty path. A workspace target that was asked for and not
delivered refuses the session, never silently falls back to the main checkout. Same rule
[[Container Isolation]] applies to isolation, and for a sharper reason: an agent that
quietly works on `main` when it was told `feat/x` commits there.

## The worktree plugin

Shells out to `git` directly, mirroring how `oci` shells out to `podman` — "zero Rust
docker knowledge" becomes "zero Rust git knowledge".

```lua
cru.shell.exec("git", { "-C", repo, "worktree", "add", dest, branch })
```

What this eventually deletes:

- `scm.rs` — `add_worktree`, `collect_branches`
- RPCs `scm.branches`, `scm.worktree_add`
- routes `/api/scm/branches`, `/api/scm/worktree`
- `CenterComposer.tsx:182-252` — the branch chip's orchestration, both `window.confirm`
  dialogs, `switchToCheckout`, `createBranchWorktree`

What has to be reimplemented in Lua: branch-name validation
(`validate_branch_name_extra`), the worktree destination template
(`resolve_worktree_dest`), porcelain parsing (`parse_worktree_porcelain`), branch sorting
(`sort_branches`). These are pure functions with tests; the tests port with them.

`scm.clone` is *not* in scope — cloning is a separate concern from worktrees and keeps
its Rust path.

## The ssh plugin

`SandboxExec` already nearly fits:

```lua
crucible.require_isolation{
  session     = session.id,
  plugin      = "ssh",
  exec_prefix = { "ssh", "-T", host },
  exec_suffix = {},
}
```

One gap, now closed. `exec_env_flag` assumed a launcher that repeats a flag per variable
(`-e K=V`), which is how `podman exec` works. `ssh` has no such flag; the idiom is a
positional `env K=V … cmd`. Reading "no flag" as "cannot pass environment" refused every
ACP launch over ssh, since an agent essentially always carries a key. `SandboxExec.env` is
now a three-state `SandboxEnv` — `Unsupported` (the safe default), `Flag(String)`,
`Inline` — declared from Lua as `exec_env_flag = "-e"` or `exec_env_inline = true`. Both
forms put the variables in the same place in the argv, so only the flag differs.

### What an ssh target means

**A remote directory.** That is the whole model, and keeping it that small is what makes the
rest fall out. `ssh:build-box` names a machine; the workspace it runs against is a path *on
that machine*. There is no syncing, no mounting, no mirroring — the files are over there,
the process is over there, and the daemon is the only thing that is not.

Two consequences follow directly:

`build_client_config` sets `working_dir` on the **launcher** process. Irrelevant for
`podman exec`, whose prefix carries `-w`, but for ssh it would set the *local* client's
directory and leave the remote agent in its login shell's home. The prefix has to carry the
remote directory itself: `ssh -T build-box cd <dir> && env … agent`.

And a locally-resolved workspace path is meaningless remotely. `worktree.resolve` runs
daemon-side and answers with a path on *this* machine; the same string on `build-box` may
not exist, or may exist and be something else. This is not hypothetical — Cursor ships the
same shape and has the same bug, where `.cursor/worktrees.json` is not found under Remote
SSH because discovery is path-based rather than target-aware.

### ssh × worktree: provision remotely

The composition is not refused, it is *performed on the far side*. Asked for
`worktree:feat/x` on `ssh:build-box`, the ssh provider:

1. clones the repo on the remote host if it is not already there, under a configured base
   directory (`~/crucible-workspaces/<repo>`, say);
2. creates the worktree there, by the same rules the local worktree provider uses;
3. answers with the **remote** path, which the runtime prefix then `cd`s into.

Which means the workspace axis has to know that the runtime is remote — the axes stay
orthogonal in *what they mean*, but resolution is ordered: runtime first, then workspace
resolved in its context. That ordering is the one real coupling, and it is worth naming
because everything else about the two axes is independent.

Nothing implements this yet. The daemon does not guard the combination either, deliberately:
it is unreachable until an ssh provider exists, and a guard written now would be guessing at
the shape of the thing it guards.

### And containers on top, theoretically

Stacking `oci` over `ssh` is not planned and not designed. It is worth checking the shape
*could* express it, because a design that structurally forbids it is a worse design:

A container is a **post-resolution step on whichever host the workspace ended up on** —
check the registry, pull or build, then wrap the command. In prefix terms that is
composition: `ssh -T build-box` followed by `podman exec -i -w <remote dir>`, with each
provider contributing its own segment rather than one provider knowing about the other. The
`SandboxExec` shape already concatenates prefix, env, suffix; nesting is the same operation
applied twice.

What would have to change: the runtime axis currently takes exactly one provider, and
`session.isolation` carries exactly one addressed target. Making it a list is the whole
structural difference. **Not doing it** — one runtime provider wins — but the reason is
scope, not a wall.

## Prior art

A survey of Claude Code, Cursor 3.x, Zed/ACP, VS Code agent plugins, OpenCode, Continue.dev,
Aider and Pi (`earendil-works/pi`) turned up **no system that models workspace-location and
process-location as separate composable axes**, and none where a plugin enumerates run
targets on demand:

| Tool | Workspace axis | Runtime axis | Who supplies the targets |
|------|---------------|--------------|--------------------------|
| Claude Code | worktrees (`--worktree`) | none — always local | CLI flag, static `.worktreeinclude` |
| Cursor 3.x | worktrees | cloud / SSH / local | built-in UI; static `.cursor/worktrees.json` |
| Zed (ACP) | none | none | — |
| VS Code, Continue, Aider, OpenCode | none | implicit local | static manifests / config files |

Cursor is closest, but treats the two as **correlated pairs** — "run this agent in the
cloud", "run it on that SSH host" — chosen per session from a built-in picker, not
contributed by anything.

Two consequences worth holding onto. First, there is nobody to copy: the sharp edges will be
found here rather than read about, and the ssh hazard above is the first one. Second, the
survey is a reason to keep the axes *cheap to collapse* if the composition turns out not to
earn its keep — the evidence that anyone wants `worktree × ssh` is currently zero, and
`worktree × container` is the only crossing with a real user behind it.

## Web

`ChipSelect` gains `children?: ChipOption[]` and a drill-down, so `Remote Machines ▸`
opens a submenu instead of flattening into a group header. `group` and `icon` already
exist on `ChipOption`.

The chip row becomes: project · **workspace target** · **runtime target** · agent · model.
The workspace chip replaces today's branch chip; the runtime chip replaces today's
isolation toggle. Both are built from published providers rather than from TSX literals,
which is the whole point — the branch chip is currently the last place in the composer
where the daemon's features are hardcoded into the frontend.

`api.ts` gains a `runPluginCommand` wrapper. `plugin.run_command` has existed on the
daemon since plugins did; the web has simply never called it.

## Sequencing

1. **Groundwork** — *done*. `workspace_targets` resolution before create, the `targets`
   publication channel, `plugin.run_command` reachable from the web, `ChipSelect`
   submenus, the `SandboxEnv` extension.
2. **Worktree plugin and the composer** — *done*. `runtime/plugins/worktree/`, `oci`
   republished on the runtime axis, and both chips rebuilt on `getTargetProviders`.
3. **SSH plugin**.
4. **Retire the `scm.*` worktree RPCs** — *done*, in the same pass. `scm.branches`,
   `scm.worktree_add`, `/api/scm/branches`, `/api/scm/worktree`, `collect_branches`,
   `add_worktree` and their pure helpers are gone; `scm.clone` stays, since cloning is a
   separate concern with no plugin behind it yet. One source of truth for what a branch is,
   and it is the plugin.

### What the composer looks like now

The chip row lost one control and gained a real one. Three chips became two:

| Before | After |
|--------|-------|
| `branch ⌄` — called `scm.worktree_add` directly, confirmed with `window.confirm` | `workspace ⌄` — whatever workspace providers enumerated |
| `run on ⌄` — hardcoded, only `This machine` enabled, the other rows `disabled: true` | *(merged below)* |
| `isolation ⌄` — a toggle plus published profile names | `run on ⌄` — `This PC` plus whatever runtime providers enumerated |

The old "run on" chip and the isolation chip were always the same question asked twice, so
they are now one. A single provider on an axis flattens its targets into the menu; two or
more get a `▸` drill-down, because a submenu holding the entire menu is an extra click
rather than a drill-down.

### Addressing, and why `oci` had to change

The runtime chip sends `{ plugin, target }` rather than a bare profile name. `oci`'s
resolver previously raised on any name it did not recognise, which was correct when it was
the only plugin on the channel and fatal the moment a second one existed — an `ssh` target
would have failed the session inside `oci`. It now ignores a table addressed elsewhere and
returns `nil`, which is what makes more than one runtime provider possible at all.

Bare `true` / `false` / `"profile-name"` still work unchanged; every existing config sends
those.

## Known limits

- One runtime provider per session, so container-on-remote is not expressible today. The
  shape would take it as a list of prefixes; see above. Scope, not a wall.
- **Resolution is ordered for a remote runtime**: the workspace axis has to resolve in the
  runtime's context, or a local path is handed to a machine it means nothing on. The one
  real coupling between two otherwise independent axes. Unreachable until an ssh provider
  exists.
- The workspace axis rewrites a path; it does not sync, copy, or clean up. A worktree
  created for a session outlives it, exactly as one created by hand does.
- `session.isolation` keeps its untyped shape for backward compatibility, so a
  malformed target is a plugin-side error rather than a schema rejection.
