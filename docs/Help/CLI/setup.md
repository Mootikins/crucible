---
title: Setup Command
description: CLI reference for bootstrapping the Crucible runtime directory.
tags: [help, cli, setup]
---

# cru setup

Bootstrap the Crucible runtime directory: copy the bundled plugins and themes to a
directory Crucible reads at startup, and create a template `init.lua` if you don't have
one. Run it after installing Crucible.

## Synopsis

```
cru setup [--runtime-dir <path>] [--force]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--runtime-dir <path>` | `~/.config/crucible/runtime` | Where to write the runtime tree |
| `--force` | off | Overwrite an existing runtime directory |

## What it does

1. **Finds a source tree**: a runtime directory next to the `cru` binary (as a distro
   package might install), or `./runtime` when run from a repo checkout. If neither
   exists — the normal case for an installed release — the tree compiled into the
   binary is written out instead.
2. **Copies plugins and themes** to the target. `defaults/` is deliberately *not*
   copied: plugins and themes layer per name across runtime roots, so your copy shadows
   only what it names, but `defaults/init.lua` is a single first-hit-wins file — a copy
   would silently freeze the shipped defaults at the version you ran setup on. The
   override point for defaults is `~/.config/crucible/init.lua`, which runs after them.
3. **Creates `~/.config/crucible/init.lua`** from a commented template, only if the file
   doesn't already exist (`--force` does not touch it).

If the target already exists, the command prints a notice and exits successfully;
`--force` rewrites it with the shipped files, replacing any hand edits inside the
runtime tree. Copying a directory onto itself is refused outright.

The default target is a directory Crucible already reads, so no follow-up is needed.
A custom `--runtime-dir` prints the follow-up: set `CRUCIBLE_RUNTIME` or add the path
to `runtimepath` in your config.

## Not to be confused with

- **`cru init`** initializes a *directory* as a kiln or project (writes its `.crucible/`
  config). `cru setup` never touches your kiln or project.
- **The first-run wizard** runs automatically — before a bare `cru` or an interactive
  `cru chat` (no one-shot query, no `--record`/`--replay`), when stdin is a terminal and
  `~/.config/crucible/config.toml` does not exist. It prompts for an LLM provider, an
  API key (stored in `secrets.toml`; skipped for Ollama), an embedding backend, and a
  default kiln path,
  then writes a minimal `config.toml`. `cru setup` writes neither `config.toml` nor
  secrets; the two are disjoint, and a fresh install typically wants both.

## See Also

- [[Help/CLI/Index]] — full CLI command reference
