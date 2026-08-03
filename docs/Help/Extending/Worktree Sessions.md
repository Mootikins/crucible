---
title: Worktree Sessions
description: Start a session against a git worktree with the worktree plugin
status: implemented
tags:
  - extending
  - plugins
  - git
aliases:
  - Worktree Plugin
  - Workspace Targets
---

# Worktree Sessions

The bundled `worktree` plugin lets a session run against a branch's own checkout
instead of whatever the project directory currently has. Pick a branch when you
start the session; if it has no worktree, one is created and the session begins
there.

This is the *workspace* axis — **where the files live**. It composes with the
*runtime* axis — **where the process runs** — which is [[Container Isolation]]'s
business. A session can run in a container against a worktree, and the two are
chosen independently. See [[Workspace and Runtime Targets]] for the design.

## Using it

Nothing to enable. The plugin ships bundled and loads with defaults, and the
composer's workspace chip appears whenever the selected project is a git
repository.

- **A branch with a checkout** — the session starts there.
- **A branch without one** — a worktree is created at `{repo}/tree/{branch}`,
  and the session starts there.
- **A name no branch has** — the branch is created from `HEAD`, then the
  worktree.

There is no confirmation prompt. Picking a row labelled *new worktree* is the
confirmation, and asking twice for the same branch returns the same checkout
rather than failing — which is what makes N sessions across N worktrees a
matter of starting N sessions.

## Where worktrees go

```toml
[plugins.worktree]
template = "{repo}/tree/{branch}"
```

`{repo}` is the repository root, `{branch}` the branch name. `{branch}` keeps
its slashes, so `feat/x` nests a directory rather than flattening to `feat-x`.

A location **inside** the repository should be gitignored. It is not refused —
the template is your choice — but an un-ignored worktree shows up as untracked
in the parent checkout, and the agent's next `git status` is then full of its
own workspace. The plugin warns when it notices.

To keep them out of the repo entirely:

```toml
[plugins.worktree]
template = "~/worktrees/{branch}"
```

## From the CLI and RPC

`session.create` takes a `workspace_target` naming the provider and the target:

```
session.create { workspace = "/repo", workspace_target = "worktree:feat/x" }
```

It resolves **before** the session exists, so the session is born in that
checkout — the agent's working directory, the registered project and the
persisted workspace all point at it from the start.

A target that cannot be resolved **refuses the session**. It does not fall back
to the main checkout: an agent that quietly works on `main` when it was told
`feat/x` commits there, and nobody looks until the commits are in the wrong
place.

## What it refuses

Branch names are checked before git sees them, then checked again by
`git check-ref-format`:

| Rejected | Why |
|----------|-----|
| `-b`, anything leading with `-` | git would read it as a flag |
| `../evil`, anything with `..` | the name becomes a path component |
| `/absolute` | same |
| `back\slash` | same |
| empty | nothing to resolve |

Names reach git as argv, never as a shell string, so a branch containing a
space, a quote or a `;` is one argument however it is spelled.

## Composing with a container

Choosing a worktree and a container gives you both: the container's workspace
mount is that worktree.

One thing that follows and is easy to miss — a linked worktree's `.git` is a
*file* containing an absolute host path into the main repository, so the
container needs the common git dir mounted alongside the worktree or every git
command inside it fails. `oci` does this for you; see
[[Container Isolation#Worktrees]].

## Cleanup

None. A worktree created for a session outlives it, exactly as one created by
hand does — `git worktree remove` when you are done with it, and
`git worktree prune` after deleting one by hand.

## See Also

- [[Container Isolation]] — the runtime axis, and how the two compose
- [[Workspace and Runtime Targets]] — the design, and how to write a provider
- [[Help/Extending/Creating Plugins]] — the plugin API this is built on
