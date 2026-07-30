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

## The order

Every tool call the agent makes walks this list top to bottom. The first layer
that says **allow** or **deny** ends it; a layer with nothing to say falls
through to the next.

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
`rg foo && rm -rf /`.

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

## Underneath all of it

Four things are not part of the chain and cannot be overridden by any layer in
it:

- **Hardcoded denies** — a small set of calls the daemon refuses outright.
- **Workspace containment** — paths outside the session's workspace are refused,
  and the offending path is redacted from the error rather than echoed back.
- **The shell policy** — parsing and vetting of shell commands, independent of
  whether `bash` was permitted.
- **Plan-mode tool filtering** — plan mode removes tools from what the agent can
  see at all. A tool that is not advertised cannot be called, so no permission
  question arises.

The first three are floors. Plan mode's filtering is a floor too, but note that
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
