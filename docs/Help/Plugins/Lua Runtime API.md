---
title: Lua Runtime API
description: Complete reference for the cru.* Lua API available to daemon plugins
status: implemented
tags:
  - plugins
  - lua
  - api
  - reference
aliases:
  - Lua Plugin API
  - cru API
---

# Lua Runtime API

This page documents the `cru.*` Lua API available to plugins running inside the Crucible daemon. `cru` is the one Lua global; every module hangs off it. Note that `cru.config.get` (the app-config store) and `cru.plugin.config.get` (the plugin's own `plugins.*` TOML section) are **different functions** (see [[Help/Lua/Configuration]]).

For TUI-specific Lua APIs (Oil rendering primitives), see [[Help/Plugins/Oil Lua API]].

## Logging

### cru.log(level, message)

Log a message at the specified level. Backed by the Rust `tracing` crate.

```lua
cru.log("debug", "Detailed trace info")
cru.log("info", "Plugin loaded")
cru.log("warn", "Connection dropped, retrying")
cru.log("error", "Fatal: could not open kiln")
```

Levels: `"debug"`, `"info"`, `"warn"`, `"error"`.

## JSON

### cru.json.encode(table)

Convert a Lua table to a compact JSON string.

```lua
local str = cru.json.encode({ name = "Alice", age = 30 })
-- '{"age":30,"name":"Alice"}'
```

### cru.json.decode(string)

Parse a JSON string into a Lua table.

```lua
local tbl = cru.json.decode('{"name":"Alice","age":30}')
print(tbl.name)  -- "Alice"
```

### cru.json.array(table)

Mark a table as a JSON **list** and return it. Lua cannot tell an empty list
from an empty map, and the encoder resolves the ambiguity as a map — so an
unmarked empty list reaches a consumer as `{}` while a populated one is
`[...]`. Tools returning result lists need the type to stay stable across
"found nothing":

```lua
local results = cru.json.array({})
-- encodes as [] rather than {}
```

For more advanced data handling (YAML, TOML, TOON, jq queries), see the `oq` module registered as `cru.oq`.

## Timer

Async timing primitives backed by `tokio::time`.

### cru.timer.sleep(seconds)

Async sleep. Yields the Lua coroutine without blocking the tokio runtime.

```lua
cru.timer.sleep(2.5)  -- yields for 2.5 seconds
```

The argument must be a finite non-negative number. Passing a negative or non-finite value raises an error.

### cru.timer.timeout(seconds, fn)

Run `fn` with a deadline. Returns a `(ok, result)` tuple:

- `(true, result)` -- function completed successfully
- `(false, error_string)` -- function raised an error
- `(false, "timeout")` -- deadline expired

```lua
local ok, result = cru.timer.timeout(5.0, function()
    return cru.http.get("https://api.example.com/data")
end)

if not ok and result == "timeout" then
    cru.log("warn", "Request timed out")
end
```

### cru.timer.clock()

Returns monotonic wall-clock time in seconds (f64) since the Lua runtime started. Unlike `os.clock()` which measures CPU time, this measures wall time that advances even when the VM is yielded at async points.

```lua
local start = cru.timer.clock()
do_work()
local elapsed = cru.timer.clock() - start
cru.log("info", string.format("Took %.2fs", elapsed))
```

## Async Task Spawning

### cru.timer.spawn(fn)

Spawn `fn` as an independent async tokio task (fire-and-forget). The function runs concurrently with the caller. Only available in daemon context when the `send` feature is enabled (`mlua/send`).

Formerly `cru.spawn`, which is removed — it lives beside `cru.timer.sleep`, the module that owns yielding.

```lua
cru.timer.spawn(function()
    cru.timer.sleep(5)
    cru.log("info", "Background task done")
end)
```

This is primarily needed when gateway event handlers (which run under `pcall`) need to call async functions that yield, such as `cru.session.subscribe()`. Since `pcall`/`xpcall` create a yield barrier in Lua, the async work must be moved to a separate task.

Errors in the spawned function are logged as warnings but do not propagate to the caller.

## Shell

Execute external commands with policy enforcement. Also available as the global `shell` for brevity. All calls are async — they yield without blocking the runtime.

Dangerous commands (`rm`, `sudo`, `chmod`, `chown`) are blocked by default. The OCI plugin and other container-runtime plugins are the expected consumers; for ad-hoc scripting, prefer targeted MCP tools over direct shell access.

### cru.shell.exec(cmd, args, opts?)

Run a command and wait for it to finish.

```lua
local r = cru.shell.exec("git", { "status", "--short" })
if r.success then
  cru.log("info", r.stdout)
end
```

**Arguments:**
- `cmd` (string) — executable name or path
- `args` (table of strings) — command-line arguments
- `opts` (table, optional):
  - `cwd` (string) — working directory
  - `env` (table) — additional environment variables as key/value pairs
  - `stdin` (string) — data to pipe to the process's stdin
  - `timeout` (number) — SECONDS to wait before the call raises. Fractions
    work: `0.5` is half a second, not zero. A negative or infinite value is
    refused rather than ignored.

**Returns a table:**
- `success` (bool) — `true` if exit code was 0
- `exit_code` (integer)
- `stdout` (string)
- `stderr` (string)

`timeout` is in **seconds**, and the default shell policy sets no deadline at
all — a call with no `timeout` runs to completion (use `cru.shell.spawn` for
streaming output from long-running work). Where a policy also sets one, the
shorter of the two wins: a plugin may shorten its own deadline and may not
lengthen the sandbox's. A call that outlives its deadline raises.

`timeout` was accepted and silently discarded until 2026-08-30. A plugin that
set one got no deadline whatever, because the deadline came from the policy
alone.

### cru.shell.spawn(cmd, args, opts?)

Like `exec`, but streams output as it arrives. `opts` takes `cwd`, `env`,
`timeout` (seconds, bounded by the policy as above) and
above, plus `on_line(stream, line)` — called with `"stdout"` or `"stderr"` and
each line as it is produced. Returns the same result table as `exec`. Useful
for long-running commands (an image build, say) that should report progress
instead of going silent.

```lua
cru.shell.spawn("docker", { "build", "." }, {
  on_line = function(stream, line)
    cru.log("info", line)
  end,
})
```

### cru.shell.which(cmd)

Return the full path to `cmd` if it exists in `PATH`, else `nil`. Synchronous.

```lua
if cru.shell.which("docker") then
  -- docker is available
end
```

## HTTP

HTTP client backed by `reqwest`. All methods are async. The default timeout is 30 seconds.

### Convenience methods

```lua
local resp = cru.http.get(url, opts?)
local resp = cru.http.post(url, opts?)
local resp = cru.http.put(url, opts?)
local resp = cru.http.patch(url, opts?)
local resp = cru.http.delete(url, opts?)
```

### cru.http.request(opts)

Full control over the request.

```lua
local resp = cru.http.request({
    url = "https://api.example.com/resource",
    method = "PUT",
    headers = { Authorization = "Bearer token123" },
    body = cru.json.encode({ key = "value" }),
    timeout = 60,
})
```

### Options table

| Field | Type | Description |
|-------|------|-------------|
| `headers` | table | Key-value pairs for request headers |
| `body` | string | Request body |
| `timeout` | number | Timeout in seconds (default: 30) |

### Response table

All HTTP methods return a response table:

| Field | Type | Description |
|-------|------|-------------|
| `status` | number | HTTP status code (0 on connection error) |
| `ok` | boolean | `true` if status is 2xx |
| `headers` | table | Response headers as key-value pairs |
| `body` | string | Response body |
| `error` | string | Error message (only present on connection failure) |

```lua
local resp = cru.http.get("https://api.example.com/users")
if resp.ok then
    local users = cru.json.decode(resp.body)
else
    cru.log("warn", "HTTP " .. resp.status .. ": " .. resp.body)
end
```

## WebSocket

WebSocket client for persistent bidirectional connections.

### cru.ws.connect(url, opts?)

Connect to a WebSocket server. Returns a connection userdata object. Raises an error on failure.

```lua
local ws = cru.ws.connect("wss://gateway.discord.gg/?v=10&encoding=json")
```

**Options:**

| Field | Type | Description |
|-------|------|-------------|
| `timeout` | number | Connection timeout in seconds (default: 30) |

### ws:send(message)

Send a text message. Raises an error if the connection is closed.

```lua
ws:send(cru.json.encode({ op = 1, d = nil }))
```

### ws:send_binary(base64_data)

Send a binary message. The payload must be base64-encoded. Raises an error if the connection is closed.

### ws:receive(timeout_secs?)

Receive the next message. Yields until a message arrives. Returns `nil` on timeout (if `timeout_secs` is provided). Raises an error if the connection is closed or encounters a protocol error.

Returns a table:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | `"text"`, `"binary"`, or `"close"` |
| `data` | string | Message content (base64-encoded for binary) |

Ping frames are handled automatically (pong is sent back). Pong frames are silently consumed.

```lua
while true do
    local msg = ws:receive(30.0)
    if msg == nil then
        -- timeout, send heartbeat or check state
    elseif msg.type == "text" then
        local payload = cru.json.decode(msg.data)
        handle_payload(payload)
    elseif msg.type == "close" then
        break
    end
end
```

### ws:close()

Close the connection. Sends a close frame with code 1000 (Normal). Idempotent: calling close on an already-closed connection is safe.

```lua
ws:close()
```

## Sessions

The `cru.session` module provides daemon-backed session management for Lua plugins. It enables plugins to create agent sessions, send messages, and receive streaming responses.

All functions are async and follow the convention of returning `(result, nil)` on success or `(nil, error_string)` on failure. Without a daemon connection, all calls return `(nil, "no daemon connected")`.

The trait is defined in `crucible-lua` as `DaemonSessionApi` and implemented by the daemon crate, avoiding a circular dependency.

> **Renamed from `cru.sessions`.** The module is singular now — `list` is the
> only plural-returning verb. `cru.sessions` still works as a deprecated alias
> that forwards to the same functions and warns once per VM; migrate when you
> can, it will be removed.

### Session handles

`create`, `get`, `list` and `fork` return **session handles** (userdata), not
plain tables. A handle carries the daemon's own response object, so every
field the old plain table exposed (`session.id`, `session.state`,
`session.kilns`, …) reads the same. On top of that:

- Every session-scoped function also exists as a **method** on the handle,
  calling the same implementation — `s:send_message("…")` is
  `cru.session.send_message(s.id, "…")`:
  `configure_agent`, `send_message`, `cancel`, `pause`, `resume`,
  `end_session`, `interaction_respond`, `subscribe`, `unsubscribe`,
  `send_and_collect`, `inject`, `messages`, `fork`, `cache_stats`, `complete`,
  `set_output_validation`, `undo`, `can_undo`, `undo_depth`, `undo_history`,
  `review_list_hunks`, `review_set_state`, `review_comment`,
  `review_resolve_comment`.
- On the *current session's* handle (`cru.session.current()`), the live config
  `s:get_variable(k)`. These need the per-session RPC binding; on a handle
  from `create`/`get`/`list` they report not-connected, and config changes go
  through `s:configure_agent(...)` instead.

```lua
local s, err = cru.session.create({ type = "chat", kilns = { "notes" } })
local response_id, err = s:send_message("summarize today's notes")
local ok, err = s:end_session()
```

`cru.session.current()` returns the session the VM is executing for (the old
spelling `cru.get_session()` still works and reads the same binding); it
errors with "No active session" when none is bound.

### cru.session.create(opts)

Create a new session. Returns a session handle whose fields read like the old plain table: at least `{ id, session_type, state, kilns }`.

`kilns` is the session's whole knowledge scope — a flat set with no primary
member, and each member is the **name** of a `kilns` entry in the user's
config, not a directory. A name no entry claims is refused rather than
attached, and a `kilns` list that is non-empty but names only unknown kilns is
an error rather than "no scope". Omit it (or pass an empty table) for a
tools-only session with no note tools, precognition, or semantic search.
**`kiln` and `connect_kilns` are no longer accepted and are ignored without
error**, so a caller still passing either silently gets the default set.

Position carries the only meaning left: the first member is where the session
writes. `workspace` stays a path — workspaces have no registry to resolve a
name against.

```lua
local session, err = cru.session.create({
    type = "chat",                            -- session type (default: "chat")
    kilns = { "notes", "reference" },         -- knowledge scope, by NAME (optional; omitted = none)
    workspace = "/path/to/workspace",         -- workspace path (optional)
    agent_card = "researcher",                -- agent card to run the session as (optional)
    tool_policy = { bash = "deny" },          -- per-tool allow/ask/deny (optional)
})
```

`type` is one of `chat`, `agent`, `workflow` and `plugin`. A `plugin` session
is one a plugin starts for its own work, such as a reflection review or a
consolidation pass. It is never a user's conversation: the reflection plugin
does not review it, and the consolidation sample leaves it out.

The options table is passed through to the daemon's `session.create` whole, so
every field that RPC accepts is available here — `isolation`, `recording_mode`,
`provider`/`model`/`endpoint` overrides, and `agent_card`. Naming any agent
field implies `configure_agent = true`, so the daemon resolves and attaches the
agent as part of create; pass `configure_agent = false` to opt out and configure
it yourself afterwards.

`agent_card` names a card from `<kiln>/.crucible/agents/` (or the workspace, or
`~/.config/crucible/agents/`). An unknown name is an error and no session is
created. `agent_name` selects an *ACP profile* and requires `agent_type = "acp"`;
setting both `agent_card` and `agent_name` is refused.

`tool_policy` is applied last, over a card's own `tools:` block. Set it here
rather than with a follow-up `configure_agent`: that call writes the *whole*
agent, so it would replace a card's prompt and model with whatever else you
passed.

Also accepts a string for the legacy positional form: `cru.session.create("chat")`.

#### Delegated creates

`delegate = true` turns the create into a delegation spawn through the
daemon's `DelegationService` — the same machinery the `delegate_session`
tool uses, with the same gates:

```lua
local job, err = cru.session.create({
    delegate = true,
    prompt = "fix the failing tests",   -- required: the child's task
    target = "cursor",                  -- optional: ACP profile or agent card
    description = "CI is red",          -- optional: becomes the child's title
})
-- job = { delegation_id = "...", child_session_id = "...", status = "spawned" }

local results, err = cru.session.collect_subagents({ job.delegation_id }, 120)
```

Three properties hold by construction:

- **Parentage is stamped, never supplied.** The daemon writes
  `parent_session_id` from the session your Lua is executing for; a
  `parent_session_id` you put in the options table is stripped before the
  boundary, on every create. Borrowing another session's delegation
  allowlist is not sayable from Lua.
- **The parent's config is the gate.** The parent session's
  `delegation_config` decides: `enabled` must be true, a named `target`
  must be inside `allowed_targets` when that list exists, and the spawn
  itself goes through the same service that enforces depth limits,
  concurrency permits and child isolation.
- **One polling surface.** `delegation_id` is a `collect_subagents` job id,
  so waiting on a delegation and waiting on any other subagent job is the
  same call.

`delegate = true` needs a current session on the VM. A bound session (a
session's own Lua) and `lua.init_session` runtimes have one; the shared
plugin VM does not, and a delegate there is refused with that reason —
spawn a plain session instead, or move the call into the session's Lua.

### cru.session.get(session_id)

Get a session by ID. Returns a session handle or `(nil, nil)` if not found.

```lua
local session, err = cru.session.get("chat-2025-01-01T0000-abc123")
if session then
    print(session.id, session.state)
end
```

### cru.session.list()

List all sessions. Returns an array of session handles.

```lua
local sessions, err = cru.session.list()
for _, s in ipairs(sessions) do
    print(s.id, s.session_type, s.state)
end
```

### cru.session.configure_agent(session_id, config)

Configure the agent for a session. The `config` table matches `SessionAgent` fields.

```lua
cru.session.configure_agent(session_id, {
    model = "claude-sonnet-4-20250514",
    system_prompt = "You are a helpful assistant for a Discord server.",
})
```

Returns `(true, nil)` on success.

### cru.session.send_message(session_id, content)

Send a user message to a session, triggering agent processing. Returns a request/response ID for tracking.

```lua
local msg_id, err = cru.session.send_message(session_id, "What is Crucible?")
```

### cru.session.send_and_collect(session_id, content, opts)

Send a message and read the reply back as a stream of parts, rather than
subscribing to the raw event bus and filtering it yourself. Returns an iterator
that yields one part at a time and `nil` when the turn ends.

Each part is a table with a `type`: `text`, `tool_call`, `tool_result`,
`thinking`, or `permission_request`.

```lua
local next_part, err = cru.session.send_and_collect(session_id, "What is Crucible?", {
    timeout = 120,              -- seconds to wait for the turn (default 120)
    max_tool_result_len = 500,  -- truncate tool output at this many chars
    interactive = false,        -- see below; default false
})

for part in next_part do
    if part.type == "text" then render(part.content) end
end
```

`interactive` decides whether an `Ask` permission decision reaches you as a
`permission_request` part or is converted straight to a denial. Leaving it
false is right for almost every plugin. Setting it true is an assertion about
your own channel — see the warning under
[Full subscribe/respond pattern](#full-subscriberespond-pattern).

### cru.session.complete(session_id, opts)

Run **one** completion against the session's own model and get the text back.
No tools, no history, nothing written to the session — this asks the model a
question *about* a session rather than taking a turn in it.

```lua
local text, err = cru.session.complete(session_id, {
    prompt  = "User: how do I open a kiln?",  -- required
    system  = "You name conversations.",      -- optional
    timeout = 20,                             -- seconds; default 30
})
```

`opts` may also be a bare string, which is the prompt. On failure it returns
`(nil, reason)` like every other `cru.session` function; a session with no
agent configured is one such failure.

The bundled `auto-title` plugin is built on this: it owns the prompt, clips
the exchange, sanitizes the answer, and the daemon persists whatever comes
back.

### cru.session.subscribe(session_id)

Subscribe to session events. Returns a `next_event` iterator function.

Calling `next_event()` yields until the next event arrives. Returns `(event_table, nil)` for each event, or `(nil, nil)` when the stream ends.

```lua
local next_event, err = cru.session.subscribe(session_id)
if not next_event then
    cru.log("warn", "Subscribe failed: " .. tostring(err))
    return
end

while true do
    local event = next_event()
    if not event then break end
    -- event.type, event.data, event.session_id
end
```

**Event types include:** `text_delta`, `message_complete`, `response_complete`, `response_done`, `stream_end`, `error`.

A `text_delta` event has `event.data.text` (or `event.data.content`) containing the text chunk.

### cru.session.unsubscribe(session_id)

Unsubscribe from session events. Returns `(true, nil)` on success.

```lua
cru.session.unsubscribe(session_id)
```

### cru.session.cancel(session_id)

Cancel the current operation in a session. Returns `(true/false, nil)` indicating whether something was cancelled.

```lua
local cancelled, err = cru.session.cancel(session_id)
```

### cru.session.pause(session_id)

Pause a session. Returns `(true, nil)` on success.

### cru.session.resume(session_id)

Resume a paused session. Returns `(true, nil)` on success.

### cru.session.end_session(session_id)

End a session permanently. Returns `(true, nil)` on success.

```lua
cru.session.end_session(session_id)
```

### cru.session.interaction_respond(session_id, request_id, response)

Respond to a permission or interaction request. The `response` table is passed through as JSON to the daemon.

The key is `allowed`, not `approved`: the daemon deserializes the table into
`PermResponse`, whose only required field is `allowed`
(`crates/crucible-core/src/interaction/permission.rs`). `approved = true` parses
as an unknown key and the request is rejected for the missing field.

```lua
cru.session.interaction_respond(session_id, request_id, { allowed = true })
```

### Full subscribe/respond pattern

Subscribe *before* sending the message to avoid missing early events:

> [!warning] Off by default, and opting in is an assertion
> A plugin's turns run non-interactively unless it says otherwise, so
> `PermissionEngine::evaluate` converts an `Ask` decision to `Deny` and the tool
> returns an error before `interaction_requested` is ever emitted. Subscribing
> alone will not surface a permission request: a rule that would have asked
> simply denies.
>
> Pass `interactive = true` in `send_and_collect`'s options to receive them.
> Doing so asserts that **exactly one identified principal** can answer — the
> daemon cannot check this, because permissions are keyed on
> `(session_id, permission_id)` alone, so wherever more than one person can
> reply the first answer binds everyone. A direct message from an account the
> operator named is the shape that holds; a shared channel is not. The Discord
> plugin's `ask` tier is the worked example.

```lua
-- 1. Subscribe first
local next_event, err = cru.session.subscribe(session_id)
if not next_event then return nil, err end

-- 2. Send the message (triggers agent processing)
local msg_id, err = cru.session.send_message(session_id, user_message)
if not msg_id then
    pcall(cru.session.unsubscribe, session_id)
    return nil, err
end

-- 3. Collect streaming response
local parts = {}
while true do
    local event = next_event()
    if not event then break end

    if event.type == "text_delta" then
        local text = event.data and event.data.text
        if text then table.insert(parts, text) end
    elseif event.type == "message_complete" or event.type == "response_done" then
        break
    elseif event.type == "error" then
        break
    end
end

-- 4. Clean up
pcall(cru.session.unsubscribe, session_id)
local response = table.concat(parts)
```

## Calling Tools

The `cru.tools` module runs workspace tools from a plugin, and decides which tools a session offers its model. Every function returns `(result, nil)` or `(nil, error_string)`.

| Function | Does |
|---|---|
| `cru.tools.call(name, args, opts?)` | run one tool; `opts.session` states which session the call is for |
| `cru.tools.batch(calls, opts?)` | run several concurrently, one result entry each |
| `cru.tools.list()` | name, description and parameters of every workspace tool |
| `cru.tools.set_active(session_id, names)` | narrow the tools that session offers, or clear the narrowing |
| `cru.tools.get_active(session_id)` | the patterns in force, or `nil` |

`call` and `batch` are checked against the operator's `permissions` rules before anything runs. See [[permissions]] for what a Lua call may do without a prompt.

### cru.tools.set_active(session_id, names)

```lua
cru.tools.set_active(ctx.session_id, { "read_*", "grep_notes" })  -- narrow
cru.tools.set_active(ctx.session_id, {})                          -- offer nothing
cru.tools.set_active(ctx.session_id, nil)                         -- back to automatic
```

`names` is an array of glob patterns — the same language a mode's `tools` selector and `cru.on`'s `pattern` speak (`*`, `?`, `[a-z]`, `{a,b}`). `nil` clears the set. An empty table is **not** a clear: it is a set that names nothing, so the session offers no tools. It must be an *array*: a map (`{ read_file = true }`) or a table with a gap in its indices is an error, not an empty set.

The set survives until it is cleared or the session ends. It is **not persisted** — it lives in the running daemon, so a daemon restart drops it and a resumed session comes back with its automatic tool list. Re-apply it from a `session:start` hook if it has to outlive the daemon.

It returns an error, rather than reporting success, when `session_id` names no live session and when it names a session delegated to an external ACP agent.

### The set only ever narrows

An active set is intersected with what the session already offers. It is applied after the session's mode filter, so:

- it **cannot re-add** a tool the mode removed. `set_active` naming `edit_file` in plan mode still gets no `edit_file` — the operator owns the floor, and a plugin may only cut below it.
- it applies only to sessions Crucible builds the tool list for. An external ACP agent brings its own file and shell tools and Crucible serves it the kiln surface over MCP beside them, so narrowing would cover one half and leave the other whole. `set_active` refuses an ACP session rather than reporting a control it does not have.

### How it interacts with progressive tool disclosure

Progressive tool disclosure defers tools automatically when their schemas would eat more than 15% of the session's context budget. The active set is applied **before** that decision, which gives two rules worth knowing:

1. Narrowing shrinks the attached schemas, so a small active set usually takes the session back under the budget and nothing is deferred at all.
2. If what remains is still over budget, deferral still happens. An active set is not an override of the context budget. Nothing is lost by that: a deferred tool stays callable through `discover_tools` → `get_tool_schema` → `invoke_tool`, so the active set decides *which* tools a session has and disclosure decides *how* they are presented.

`discover_tools`, `get_tool_schema` and `invoke_tool` are never hidden by an active set — they are how a deferred tool is reached, not tools of the session's own.

### It is enforced at dispatch, not only advertised

A tool outside the active set is refused when the model calls it anyway, with a message naming the plugin narrowing. Filtering only the advertised list would leave every excluded tool runnable by a model that names one from earlier context or through `invoke_tool`.

One gap worth knowing: `discover_tools` and `get_tool_schema` search the whole catalog and are not filtered by the set, so an excluded tool can still be **found** there. It cannot be run — the dispatch refusal above still applies.

### cru.tools.get_active(session_id)

```lua
local names, err = cru.tools.get_active(session_id)
if err then return end          -- a real failure
if not names then return end    -- no explicit set: whatever the mode allows
```

Three outcomes, not two. `(nil, nil)` is a **successful** answer meaning no set is in force, which is the common case; check the error before concluding anything from a `nil` first return. What comes back is the patterns that were set, not the tool names they expand to.

## Asking the User

The `cru.ui` module asks whichever client is attached to a session, and waits for the answer. Every function takes `(session_id, opts)` and returns `(response, nil)` or `(nil, error_string)`.

There is one function per `InteractionRequest` variant, and the set is closed:

| Function | Shows | Answers with |
|---|---|---|
| `cru.ui.ask` | one question, optional choices | `{ selected = {…}, other = "…" }` |
| `cru.ui.ask_batch` | one to four questions together | `{ answers = { … }, cancelled = bool }` |
| `cru.ui.edit` | an editable text box | `{ modified = "…" }` |
| `cru.ui.show` | content, no question | `{ kind = "cancelled" }` on dismiss |
| `cru.ui.permission` | a permission prompt | `{ allowed = bool, scope = "…" }` |
| `cru.ui.popup` | a list with labels and descriptions | `{ selected_index = n }` or `{ other = "…" }` |
| `cru.ui.panel` | a filterable, multi-select list | `{ selected = {…}, cancelled = bool }` |

```lua
local answer = cru.ui.ask(session_id, {
  question = "Which branch should I use?",
  choices = { "main", "develop" },
  allow_other = true,
})

if answer.kind == "cancelled" then
  return  -- nobody answered
end
```

The options table is the variant's own fields, passed through unchanged. A `kind` key in it is ignored — the function name already chose the variant.

### Always handle `cancelled`

A response of `{ kind = "cancelled" }` is a **successful** call that nobody answered. It happens when no client is attached, when the user dismisses the modal, and when the timeout elapses. On a headless daemon it is the common case, not the exception. It is a value to inspect, never an error to `pcall` around.

### Timeout

`opts.timeout` is seconds to wait, default `300` — the same wait the permission prompt uses. A `timeout` of `0` falls back to the default rather than giving up before asking. The key is consumed by the binding and never reaches the request.

### Which client answers

The request goes to every client attached to the session, and the first answer wins. Two clients are not serialized against each other, and `cru.ui` deliberately does **not** queue behind permission prompts: two plugins asking unrelated questions must not block each other, and a plugin that asks from inside a permission handler would otherwise deadlock.

## Conversation Context

The `cru.context` module manipulates a session's conversation context. All daemon-backed functions take an explicit `session_id` and return `(result, nil)` or `(nil, error_string)`; until the daemon wires the session API they are stubs returning `(nil, "no daemon connected")`. `estimate_tokens` is pure and always works. `cru.context.attach` is also registered on the per-session VMs, so `cru.on` handlers can call it regardless of which VM they run in.

### cru.context.estimate_tokens(text)

Pure helper — no daemon needed. Returns a rough estimate: byte length divided by 4, rounded up — `"hello world"` (11 bytes) estimates to 3.

### cru.context.usage(session_id)

Returns `({ messages, prompt_tokens, budget, percent }, nil)` on success.

```lua
local u, err = cru.context.usage(session_id)
if u and u.percent > 0.8 then cru.context.compact(session_id) end
```

### cru.context.compact(session_id)

Compact the session's context. Returns `(true, nil)` on success.

### cru.context.messages(session_id, opts?)

Load conversation messages. `opts`: `{ role = "user"|"assistant"|"system", limit = N, tools = true }`. A thin alias over the same daemon call as `cru.session.messages` — identical semantics, kept here so context-manipulating code can stay inside one namespace.

Each row is `{ role, content, timestamp }`. `tools = true` adds two more row shapes:

- `{ role = "tool_call", id, name, args, timestamp }` — `args` is the argument table the agent sent.
- `{ role = "tool_result", id, content, truncated, error?, timestamp }` — `id` matches the `tool_call` row. `error` is present only when the tool failed.

A `role` filter names a text role, so it excludes the tool rows even when `tools = true`.

```lua
local rows = cru.session.messages(session_id, { tools = true })
for _, row in ipairs(rows) do
  if row.role == "tool_result" and row.error then
    print("ERROR: " .. row.error)
  end
end
```

### cru.context.remove(session_id, range)

Remove messages. `range` is one of `{ type = "all" }`, `{ type = "last"|"first", n = N }`, or `{ type = "indices", start = S, ["end"] = E }`. Returns `(count_removed, nil)`.

### cru.context.attach(session_id, content, opts?)

Queue retrieved content for the session's **next LLM call**. Context only: attachments never reach the conversation tree or the session log — one turn's context, then gone.

```lua
cru.on("tool_result", { pattern = "read_file" }, function(ctx, event)
  local ft = event.args.path:match("%.(%w+)$")
  if not ft then return end
  local kiln = cru.kiln.active
  if not kiln then return end
  local hits = cru.kiln.search(kiln, cru.embed(kiln, "conventions for " .. ft), 3)
  local paths = {}
  for _, hit in ipairs(hits) do paths[#paths + 1] = hit.path end
  cru.context.attach(ctx.session_id, table.concat(paths, "\n"), { key = "filetype:" .. ft })
end)
```

Returns `(true, nil)` when queued, `(false, reason)` when dropped. Dropping is normal operation, not an error:

- **Duplicate key** — `opts.key` deduplicates for the whole session (surviving drains), so a handler firing on every tool call attaches once.
- **Budget exhausted** — a cumulative 2000-character budget per session, spent permanently. Deliberately tight: every attached character is re-sent on each subsequent LLM call.
- **Empty content.**

### cru.context.register_validator(name, fn)

Register a named output validator. `fn` receives the agent's text response and returns `true`, `false`, or `(false, reason)`. A validator runs when a session agent's `output_validation` is set to `lua:<name>` — via `cru.session.set_output_validation(session_id, "lua:<name>")` (which also accepts the table form `{ type = "lua", name = "<name>" }`) or the `session.set_output_validation` RPC; on failure the reason is fed back to the agent for retry (`validation_retries`, default 3). A non-boolean or missing first return value counts as a failure with a descriptive reason, as does naming a validator that was never registered.

```lua
cru.context.register_validator("has_sources", function(text)
  if text:match("%[%[") then return true end
  return false, "response cites no notes"
end)
```

Registered at plugin load, before the daemon-backed `cru.context` methods are wired — so registering validators from a plugin's `init.lua` works.

## Rate Limiting

### cru.ratelimit.new(opts)

Create a token bucket rate limiter. Returns a limiter userdata object.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `capacity` | number | 5 | Maximum number of tokens |
| `interval` | number | 1.0 | Seconds per token refill |

Both must be finite positive numbers.

```lua
local limiter = cru.ratelimit.new({ capacity = 5, interval = 1.0 })
```

### limiter:acquire()

Async: yields until a token is available. Use this for automatic backpressure.

```lua
limiter:acquire()
cru.http.post(url, { body = payload })
```

### limiter:try_acquire()

Synchronous: returns `true` if a token was immediately available, `false` otherwise.

```lua
if limiter:try_acquire() then
    send_request()
else
    cru.log("info", "Rate limited, skipping")
end
```

### limiter:remaining()

Synchronous: returns the current token count (number).

## Retry

### cru.retry(fn, opts)

Execute `fn` with exponential backoff on failure. Implemented in pure Lua on top of `cru.timer.sleep`.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_retries` | number | 3 | Maximum retry attempts |
| `base_delay` | number | 1.0 | Initial delay in seconds |
| `max_delay` | number | 60.0 | Maximum delay cap |
| `jitter` | boolean | true | Add random jitter to delays |
| `retryable` | function | `function() return true end` | Predicate receiving the error; return `false` to stop retrying |

If the error is a table with an `after` field, the delay is raised to at least that value (for server-specified retry-after).

Returns the result of `fn` on success. Raises the last error if all retries are exhausted or the error is not retryable.

```lua
local result = cru.retry(function()
    local resp = cru.http.get("https://api.example.com/data")
    if not resp.ok then
        error({ retryable = resp.status >= 500 })
    end
    return resp
end, {
    max_retries = 5,
    base_delay = 1.0,
    max_delay = 30.0,
    retryable = function(err)
        return type(err) == "table" and err.retryable
    end,
})
```

## Event Emitter

### cru.emitter.new()

Create a new event emitter. Implemented in pure Lua.

```lua
local events = cru.emitter.new()
```

### emitter:on(event, fn)

Register a handler for an event. Returns an ID for removal. Handlers fire in registration order.

### emitter:once(event, fn)

Register a one-shot handler that auto-removes after the first call.

### emitter:off(event, id)

Remove a handler by event name and ID.

### emitter:off_all(event?)

Remove all handlers for an event, or all handlers entirely if no event is specified.

### emitter:emit(event, ...)

Fire all handlers for the event with the given arguments. Handler errors are caught with `pcall` and logged via `cru.log("warn", ...)` without stopping other handlers.

```lua
local events = cru.emitter.new()

events:on("message", function(data)
    cru.log("info", "Got message: " .. data.content)
end)

events:emit("message", { content = "Hello" })
```

## Argument Validation

### cru.check

Validation functions for plugin arguments. All support an optional `opts` table with `{ optional = true }` to allow `nil` values. On failure, they raise an error with a descriptive message.

```lua
cru.check.string(val, "name")
cru.check.string(val, "name", { optional = true })
cru.check.number(val, "count", { min = 1, max = 100 })
cru.check.boolean(val, "enabled")
cru.check.table(val, "options")
cru.check.func(val, "callback")
cru.check.one_of(val, { "json", "text", "yaml" }, "format")
```

## Plugin Services

Plugins can declare long-running services that the daemon spawns automatically after plugin initialization. Each service is a function that runs as an independent async task.

Services are declared in the plugin's spec table (returned from `init.lua`):

```lua
return {
    name = "my-plugin",
    version = "1.0.0",
    capabilities = { "network", "agent" },

    services = {
        my_service = {
            desc = "Description of what this service does",
            fn = function()
                -- Long-running loop
                while true do
                    do_work()
                    cru.timer.sleep(60)
                end
            end,
        },
    },

    tools = { ... },
    commands = { ... },
}
```

Each entry in `services` has:

| Field | Type | Description |
|-------|------|-------------|
| `desc` | string | Human-readable description |
| `fn` | function | The service function (runs as an async task) |

The daemon spawns each service function after the plugin's `setup()` callback completes. Services typically contain an infinite loop with a connection or polling cycle, using `cru.retry` or `cru.timer.sleep` for resilience.

**Example from the Discord plugin:**

```lua
services = {
    gateway = {
        desc = "Discord WebSocket gateway connection",
        fn = gateway.connect,
    },
},
```

The `gateway.connect` function uses `cru.retry` with reconnection backoff, `cru.ws.connect` for the WebSocket, and `cru.timer` for heartbeat scheduling.

## Supervised Services

`cru.service` is a pure-Lua supervision layer over a service's start function: retry with backoff, a status registry, and config-schema resolution. **It is not the spawn mechanism.** Only the spec-table `services` field above gets a function spawned — `cru.service.define` on its own starts nothing, and the daemon never reads `cru.service`'s registry. The two compose: `define` returns a `{ desc, fn }` table shaped exactly like a spec-table entry.

```lua
local svc = cru.service.define({
    name = "gateway",
    desc = "Discord WebSocket gateway connection",
    start = function() connect_loop() end,   -- required
    stop = function() ws:close() end,        -- optional
    health = function() return ws ~= nil end, -- optional
    restart = { max_retries = 10, base_delay = 1.0, max_delay = 60.0 }, -- defaults shown
})

return {
    name = "my-plugin",
    services = { gateway = svc },  -- this line is what gets it spawned
}
```

### cru.service.define(spec)

Validates `name`, `desc`, `start` (required) and `stop`, `health` (optional), then returns `{ desc, fn }` where `fn` wraps `start` in `cru.retry` using the `restart` settings. An error raised as a table with `retryable = false` stops the retry loop; any other error is retried. When the wrapped function finally returns or gives up, the service is marked not running and the outcome is logged.

If `spec.config` is a schema table, values are resolved **at define time**, per key. All three steps use the **service's `name`**, not the plugin's — name the service after the plugin if you want them to line up:

1. keys marked `secret = true`: the env var `CRUCIBLE_<NAME>_<KEY>` (service name and key uppercased, non-alphanumerics replaced with `_` — `name = "gateway"` reads `CRUCIBLE_GATEWAY_*`)
2. `cru.plugin.config.get("<name>.<key>")` — the `plugins.<name>` table of your `init.lua`
3. the schema's `default`

The resolved table is stored on the internal registry entry only — nothing passes it to `start`, and no accessor exposes it. A start function that needs the values must resolve them itself (the `web-search` plugin's `ws_config.lua` does exactly this, matching the env-var convention).

### cru.service.status(name) / cru.service.list()

`status` returns `nil` for an unknown name, else `{ name, desc, running, healthy }`. `list` returns the same shape for every defined service. `healthy` is `nil` when the service declared no `health` function; otherwise it is the health function's return value, with an error or falsy result reported as `false`.

### cru.service.stop(name)

Calls the service's `stop` function (errors logged, not raised), marks it not running, and returns `true`; returns `false` for an unknown name.

> [!warning] Status is self-reported, and stop does not reach the daemon
> The registry lives inside the plugin VM. `stop` invokes your `stop` callback and flips the Lua-side flag — it does not abort the daemon's spawned task. Conversely, when the daemon aborts a service task (plugin reload/disable/remove), no `stop` callback runs and the Lua wrapper never resumes, so `status` can keep reporting `running = true` for a task that is gone. Treat services as cancel-safe; treat `status` as advisory.

## Session Status

### cru.plugin.set_status(opts) / cru.plugin.clear_status(opts)

A durable, session-scoped status slot in the UI — unlike `cru.log.notify`,
which is transient and easily missed. Slots are keyed, so the TUI and web
render any plugin's slots generically; the `oci` plugin uses one to show
whether a session is sandboxed.

```lua
cru.plugin.set_status{
  session = session.id,      -- required
  key     = "oci",           -- required; one slot per key per session
  text    = "sandboxed: alpine:latest",  -- required; keep it short
  level   = "info",          -- info | warn | error (default info)
  progress = 0.4,            -- optional: fraction 0..1, or `true` for a spinner
}

cru.plugin.clear_status{ session = session.id, key = "oci" }
```

`progress = true` means indeterminate work (render a spinner); a number is a
fraction complete, clamped to 0..1. Omit it for a state that is not work
("sandboxed: alpine"). Setting empty text is not the same as clearing —
`clear_status` removes the slot. A session's slots are dropped when it ends.

## Publications

### cru.plugin.publish(key, value)

Publish data about the plugin itself for clients to render — not
session-scoped (that's what status slots are for). The daemon stores the
value verbatim as JSON and every client reads the same answer, keyed by
publication name and attributed to the publishing plugin.

```lua
cru.plugin.publish("isolation", {
  available = true,
  profiles  = { "rust", "throwaway" },
})
```

`value` must be JSON-encodable data (no functions or userdata). The publishing
plugin's name is supplied by the loader, not the caller, and a plugin's
publications are dropped when it reloads.

Some keys the daemon itself reads:

| Key | Who reads it | Shape |
|-----|--------------|-------|
| `targets` | Workspace/runtime target resolution before `session.create` | `{ axis, label, targets_command, resolve_command }` |
| `session_title` | Session titling, on the first completed turn | `{ command = "<plugin command name>" }` |

`session_title` is how `auto-title` is found — by channel, never by plugin
name, so publishing the same key replaces it. The command is called with
`{ session_id, user, assistant }` and answers `{ title = "…" }` (or a bare
string). Raising, or answering with a blank title, leaves the daemon's
truncation fallback in place.

## Options

### cru.plugin.options(tree)

Declare a settings tree that every frontend renders in its own idiom — the
settings pane in the TUI, a form on the web. The shape follows Ace3's
AceConfig options tables: nested `group` nodes whose `args` hold typed leaves,
with `get`/`set` accessors called when a value is read or written.

```lua
cru.plugin.options{
  type = "group",
  args = {
    image = {
      type = "input", name = "Image", order = 1,
      desc = "Image to run workspace tools in",
      get = function() return config.image end,
      set = function(_, v) config.image = v end,
    },
    runtime = {
      type = "select", name = "Runtime", order = 2,
      -- evaluated at render time, so only installed runtimes are offered
      values = function() return installed_runtimes() end,
    },
    rebuild = { type = "execute", name = "Rebuild image", func = rebuild },
  },
}
```

Two properties are load-bearing:

- **Any field may be a function**, evaluated when the tree is read — that is
  what lets `values` describe the current box rather than the box at load.
- **`get`/`set`/`disabled`/`hidden` inherit toward the root**, so one accessor
  at the top serves every leaf; `false` breaks inheritance on a node that
  means it.

Values written through the pane persist via the daemon's option store.

## Plugin Tools and Commands

A plugin's spec table can also declare `tools` (callable by the agent's model) and `commands` (invocable by clients, e.g. as slash commands). Both take a `desc`, an optional `params` list, and a `fn`:

```lua
return {
    name = "shout",

    tools = {
        shout = {
            desc = "Uppercase the given text",
            params = {
                { name = "text", type = "string", desc = "Text to shout" },
            },
            fn = M.shout,
        },
    },

    commands = {
        greet = {
            desc = "Greet someone",
            hint = "[name]",
            params = {
                { name = "who", type = "string", desc = "Who to greet", optional = true },
            },
            fn = M.greet,
        },
    },
}
```

A tool's `fn` receives one table of arguments and returns any JSON-representable value. `params` becomes the JSON Schema the model sees; a param is required unless marked `optional = true`.

Commands are listed over the `plugin.commands` RPC and invoked with `plugin.run_command`. `plugin.run_command` is client-initiated (a user typing `/name`), so it does not pass the model-facing permission gate — anything with socket access can invoke any plugin command; treat commands as user-facing entry points, not as a place to hide privileged operations behind. The TUI consumes both: a plugin command appears in slash autocomplete (tagged `(plugin)`) and `/name args` invokes it, with the result shown as a system message. Built-in slashes always dispatch first — a plugin cannot shadow `/plan` or `/help`. The web client does not consume commands yet.

### Name collisions

**A plugin tool whose name collides with a built-in tool is rejected, not shadowed.** Built-ins (`bash`, `read_file`, `edit_file`, `write_file`, `glob`, `grep`, the kiln MCP tools, and the tool-discovery bridge) always win; the plugin's tool is dropped with a warning in the daemon log and never advertised to the model. Rename it.

Two plugins claiming the same tool name resolve first-loaded-wins, also with a warning. Commands are a separate namespace: a command may share a name with a built-in tool.

A tool or command declared without a `fn` is not registered — declaring one the plugin doesn't export would advertise a call that always fails.

## Paths: the state() exception

`cru.paths.workspace()` and `.session()` read the `PathsContext` the module was registered with and **raise** when that path is unconfigured (they do not return `nil`). `cru.paths.state(plugin)` is different: it ignores `PathsContext` entirely and resolves against the daemon's data root at call time — `$CRUCIBLE_HOME`, else `~/.crucible` — returning `<data root>/plugin-state/<plugin>/`, created on demand. One Lua VM serves every plugin, so a state directory baked in at registration would be the same directory for all of them; instead each plugin names itself. Two consequences:

- `paths.state("my-plugin")` works even where `paths.workspace()` raises.
- There is no `paths.kiln()`: a kiln is resolved by NAME through `cru.kiln.path(name, relative?)`.
- Plugin state lives under the global data root, not inside the kiln or workspace.

`plugin` must be a single path component: `""`, `"."`, `".."`, `"a/b"`, and absolute paths are refused.

## Kiln reads by name

These functions take a kiln NAME first, never a directory. The daemon maps the name to the open kiln through its registry. The same read authority as `cru.kiln.list` applies to `note`, `notes` and `links`; `blocks` and `search` read the kiln's block table, which holds only that kiln's rows. An unregistered name raises an error that names the kiln. Before the daemon binds them, each function is a stub: `note` answers `nil`, the array functions answer an empty table, `links` answers two empty arrays, and `cru.embed` raises.

### cru.kiln.blocks(kiln, path)

The stored blocks of one note, in span order. Each row is `{ span_start, span_end, kind, vector? }`. The vector is absent when the block fell under the word floor. A path with no note answers an empty table.

### cru.kiln.note(kiln, path)

The index row of one note by its exact kiln-relative path, in the same shape `cru.kiln.get` answers with: `path`, `title`, `content_hash`, `tags`, `links_to`, `properties`, `updated_at`, `has_embedding`. `properties` holds the frontmatter, so `note.properties.description` reads a note's description. A path with no note answers `nil`.

### cru.kiln.notes(kiln, limit?)

Every index row the authority can read, in the same shape as `note`. `limit` cuts the array after that many rows. The order is the store's order, not a ranking. A strategy that builds a graph of the kiln starts here.

### cru.kiln.links(kiln, path)

The resolved links of one note in both directions: `{ outlinks = { path, ... }, backlinks = { path, ... } }`. Both arrays hold note paths, sorted, with no duplicates. A wikilink that names no note is absent, because nothing can follow it. A path with no note, or one outside the authority, answers two empty arrays.

### cru.kiln.search(kiln, vector, limit)

A dense search over the stored blocks of the kiln: the best `limit` blocks by cosine similarity to `vector`. Each row is `{ path, span_start, span_end, kind, score }`. The vector comes from `cru.embed`, so the dimension is the caller's to get right. A kiln with no block rows answers an empty table. This search runs no `search:rerank` stage, so a rerank handler can call it without recursion.

### cru.embed(kiln, text)

The vector the kiln's embedder gives `text`, as an array of numbers. It uses the provider the daemon configured for indexing, the same one the `kiln.embed_query` RPC uses, so a vector from here compares with the stored block vectors. When no embedding provider is configured, the call raises with the reason.

```lua
local kiln = "notes"
local hits = cru.kiln.search(kiln, cru.embed(kiln, "how does a session attach a kiln"), 5)
for _, hit in ipairs(hits) do
  local note = cru.kiln.note(kiln, hit.path)
  print(note and note.title, hit.span_start, hit.score)
end
```

## Kiln access and the `vault` name

The kiln API is `cru.kiln` / `cru.kiln` — there is no `cru.vault` table. The old "vault" name survives in exactly one Lua-facing place: a plugin manifest may declare `capabilities: [vault]`, which parses as the `kiln` capability. (The Rust registration functions are still named `register_vault_module*`; that is internal naming only.)

## Session-VM-only: cru.defaults and cru.modes

`cru.defaults` (session default values like `system_prompt`) and `cru.modes` (mode definitions) are registered **only on the per-session Lua VM** — and on the daemon VM, against the same stores. A write from `~/.config/crucible/init.lua` at boot therefore reaches every session, and the daemon re-applies it over each session's freshly-loaded defaults file. `cru.permissions` is the one that stays session-only.

## See Also

- [[Help/Lua/Language Basics]] -- Lua scripting overview
- [[Help/Lua/Configuration]] -- Configuration via init.lua
- [[Help/Extending/Creating Plugins]] -- Plugin development guide
- [[Help/Plugins/Oil Lua API]] -- TUI rendering primitives
- [[Help/Plugins/Vendoring Lua Dependencies]] -- ship a pure-Lua library with a plugin
