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

This page documents the `cru.*` Lua API available to plugins running inside the Crucible daemon. Modules are registered under both the `cru` and `crucible` namespaces, with two caveats: the UI-config namespaces (`crucible.colorscheme`, `crucible.ui`, `crucible.statusline`, ...) exist only under `crucible`, and `cru.config.get` / `crucible.config.get` are **different functions** (see [[Help/Lua/Configuration]]). Some modules (like `http`, `oq`, `fs`, `graph`) are also available as standalone globals for backwards compatibility.

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

### cru.spawn(fn)

Spawn `fn` as an independent async tokio task (fire-and-forget). The function runs concurrently with the caller. Only available in daemon context when the `send` feature is enabled (`mlua/send`).

```lua
cru.spawn(function()
    cru.timer.sleep(5)
    cru.log("info", "Background task done")
end)
```

This is primarily needed when gateway event handlers (which run under `pcall`) need to call async functions that yield, such as `cru.sessions.subscribe()`. Since `pcall`/`xpcall` create a yield barrier in Lua, the async work must be moved to a separate task.

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

**Returns a table:**
- `success` (bool) — `true` if exit code was 0
- `exit_code` (integer)
- `stdout` (string)
- `stderr` (string)

There is no per-call timeout option, and the default shell policy sets no deadline — commands run to completion (use `cru.shell.spawn` for streaming output from long-running work). A policy configured with a timeout raises an error for calls that outlive it.

### cru.shell.spawn(cmd, args, opts?)

Like `exec`, but streams output as it arrives. `opts` takes `cwd` and `env` as
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

The `cru.sessions` module provides daemon-backed session management for Lua plugins. It enables plugins to create agent sessions, send messages, and receive streaming responses.

All functions are async and follow the convention of returning `(result, nil)` on success or `(nil, error_string)` on failure. Without a daemon connection, all calls return `(nil, "no daemon connected")`.

The trait is defined in `crucible-lua` as `DaemonSessionApi` and implemented by the daemon crate, avoiding a circular dependency.

### cru.sessions.create(opts)

Create a new session. Returns a session table with at least `{ id, session_type, state, kilns }`.

`kilns` is the session's whole knowledge scope — a flat set with no primary
member, and each member is the **name** of a `[kilns]` entry in the user's
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
local session, err = cru.sessions.create({
    type = "chat",                            -- session type (default: "chat")
    kilns = { "notes", "reference" },         -- knowledge scope, by NAME (optional; omitted = none)
    workspace = "/path/to/workspace",         -- workspace path (optional)
    agent_card = "researcher",                -- agent card to run the session as (optional)
    tool_policy = { bash = "deny" },          -- per-tool allow/ask/deny (optional)
})
```

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

Also accepts a string for the legacy positional form: `cru.sessions.create("chat")`.

### cru.sessions.get(session_id)

Get a session by ID. Returns the session table or `(nil, nil)` if not found.

```lua
local session, err = cru.sessions.get("chat-2025-01-01T0000-abc123")
if session then
    print(session.id, session.state)
end
```

### cru.sessions.list()

List all sessions. Returns an array of session summary tables.

```lua
local sessions, err = cru.sessions.list()
for _, s in ipairs(sessions) do
    print(s.id, s.session_type, s.state)
end
```

### cru.sessions.configure_agent(session_id, config)

Configure the agent for a session. The `config` table matches `SessionAgent` fields.

```lua
cru.sessions.configure_agent(session_id, {
    model = "claude-sonnet-4-20250514",
    system_prompt = "You are a helpful assistant for a Discord server.",
})
```

Returns `(true, nil)` on success.

### cru.sessions.send_message(session_id, content)

Send a user message to a session, triggering agent processing. Returns a request/response ID for tracking.

```lua
local msg_id, err = cru.sessions.send_message(session_id, "What is Crucible?")
```

### cru.sessions.send_and_collect(session_id, content, opts)

Send a message and read the reply back as a stream of parts, rather than
subscribing to the raw event bus and filtering it yourself. Returns an iterator
that yields one part at a time and `nil` when the turn ends.

Each part is a table with a `type`: `text`, `tool_call`, `tool_result`,
`thinking`, or `permission_request`.

```lua
local next_part, err = cru.sessions.send_and_collect(session_id, "What is Crucible?", {
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

### cru.sessions.complete(session_id, opts)

Run **one** completion against the session's own model and get the text back.
No tools, no history, nothing written to the session — this asks the model a
question *about* a session rather than taking a turn in it.

```lua
local text, err = cru.sessions.complete(session_id, {
    prompt  = "User: how do I open a kiln?",  -- required
    system  = "You name conversations.",      -- optional
    timeout = 20,                             -- seconds; default 30
})
```

`opts` may also be a bare string, which is the prompt. On failure it returns
`(nil, reason)` like every other `cru.sessions` function; a session with no
agent configured is one such failure.

The bundled `auto-title` plugin is built on this: it owns the prompt, clips
the exchange, sanitizes the answer, and the daemon persists whatever comes
back.

### cru.sessions.subscribe(session_id)

Subscribe to session events. Returns a `next_event` iterator function.

Calling `next_event()` yields until the next event arrives. Returns `(event_table, nil)` for each event, or `(nil, nil)` when the stream ends.

```lua
local next_event, err = cru.sessions.subscribe(session_id)
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

### cru.sessions.unsubscribe(session_id)

Unsubscribe from session events. Returns `(true, nil)` on success.

```lua
cru.sessions.unsubscribe(session_id)
```

### cru.sessions.cancel(session_id)

Cancel the current operation in a session. Returns `(true/false, nil)` indicating whether something was cancelled.

```lua
local cancelled, err = cru.sessions.cancel(session_id)
```

### cru.sessions.pause(session_id)

Pause a session. Returns `(true, nil)` on success.

### cru.sessions.resume(session_id)

Resume a paused session. Returns `(true, nil)` on success.

### cru.sessions.end_session(session_id)

End a session permanently. Returns `(true, nil)` on success.

```lua
cru.sessions.end_session(session_id)
```

### cru.sessions.interaction_respond(session_id, request_id, response)

Respond to a permission or interaction request. The `response` table is passed through as JSON to the daemon.

The key is `allowed`, not `approved`: the daemon deserializes the table into
`PermResponse`, whose only required field is `allowed`
(`crates/crucible-core/src/interaction/permission.rs`). `approved = true` parses
as an unknown key and the request is rejected for the missing field.

```lua
cru.sessions.interaction_respond(session_id, request_id, { allowed = true })
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
local next_event, err = cru.sessions.subscribe(session_id)
if not next_event then return nil, err end

-- 2. Send the message (triggers agent processing)
local msg_id, err = cru.sessions.send_message(session_id, user_message)
if not msg_id then
    pcall(cru.sessions.unsubscribe, session_id)
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
pcall(cru.sessions.unsubscribe, session_id)
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

`call` and `batch` are checked against the operator's `[permissions]` rules before anything runs. See [[permissions]] for what a Lua call may do without a prompt.

### cru.tools.set_active(session_id, names)

```lua
cru.tools.set_active(ctx.session_id, { "read_*", "grep_notes" })  -- narrow
cru.tools.set_active(ctx.session_id, {})                          -- offer nothing
cru.tools.set_active(ctx.session_id, nil)                         -- back to automatic
```

`names` is an array of glob patterns — the same language a mode's `tools` selector and `crucible.on`'s `pattern` speak (`*`, `?`, `[a-z]`, `{a,b}`). `nil` clears the set. An empty table is **not** a clear: it is a set that names nothing, so the session offers no tools. It must be an *array*: a map (`{ read_file = true }`) or a table with a gap in its indices is an error, not an empty set.

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

The `cru.context` module manipulates a session's conversation context. All daemon-backed functions take an explicit `session_id` and return `(result, nil)` or `(nil, error_string)`; until the daemon wires the session API they are stubs returning `(nil, "no daemon connected")`. `estimate_tokens` is pure and always works. `cru.context.attach` is also registered on the per-session VMs, so `crucible.on` handlers can call it regardless of which VM they run in.

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

Load conversation messages. `opts`: `{ role = "user"|"assistant"|"system", limit = N }`. A thin alias over the same daemon call as `cru.sessions.messages` — identical semantics, kept here so context-manipulating code can stay inside one namespace.

### cru.context.remove(session_id, range)

Remove messages. `range` is one of `{ type = "all" }`, `{ type = "last"|"first", n = N }`, or `{ type = "indices", start = S, ["end"] = E }`. Returns `(count_removed, nil)`.

### cru.context.attach(session_id, content, opts?)

Queue retrieved content for the session's **next LLM call**. Context only: attachments never reach the conversation tree or the session log — one turn's context, then gone.

```lua
crucible.on("tool_result", { pattern = "read_file" }, function(ctx, event)
  local ft = event.args.path:match("%.(%w+)$")
  if not ft then return end
  local notes = cru.kiln.search("conventions for " .. ft)
  cru.context.attach(ctx.session_id, notes, { key = "filetype:" .. ft })
end)
```

Returns `(true, nil)` when queued, `(false, reason)` when dropped. Dropping is normal operation, not an error:

- **Duplicate key** — `opts.key` deduplicates for the whole session (surviving drains), so a handler firing on every tool call attaches once.
- **Budget exhausted** — a cumulative 2000-character budget per session, spent permanently. Deliberately tight: every attached character is re-sent on each subsequent LLM call.
- **Empty content.**

### cru.context.register_validator(name, fn)

Register a named output validator. `fn` receives the agent's text response and returns `true`, `false`, or `(false, reason)`. A validator runs when a session agent's `output_validation` is set to `lua:<name>` — via `cru.sessions.set_output_validation(session_id, "lua:<name>")` (which also accepts the table form `{ type = "lua", name = "<name>" }`) or the `session.set_output_validation` RPC; on failure the reason is fed back to the agent for retry (`validation_retries`, default 3). A non-boolean or missing first return value counts as a failure with a descriptive reason, as does naming a validator that was never registered.

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
2. `crucible.config.get("<name>.<key>")` — the `[plugins.<name>]` section of config.toml
3. the schema's `default`

The resolved table is stored on the internal registry entry only — nothing passes it to `start`, and no accessor exposes it. A start function that needs the values must resolve them itself (the `web-search` plugin's `ws_config.lua` does exactly this, matching the env-var convention).

### cru.service.status(name) / cru.service.list()

`status` returns `nil` for an unknown name, else `{ name, desc, running, healthy }`. `list` returns the same shape for every defined service. `healthy` is `nil` when the service declared no `health` function; otherwise it is the health function's return value, with an error or falsy result reported as `false`.

### cru.service.stop(name)

Calls the service's `stop` function (errors logged, not raised), marks it not running, and returns `true`; returns `false` for an unknown name.

> [!warning] Status is self-reported, and stop does not reach the daemon
> The registry lives inside the plugin VM. `stop` invokes your `stop` callback and flips the Lua-side flag — it does not abort the daemon's spawned task. Conversely, when the daemon aborts a service task (plugin reload/disable/remove), no `stop` callback runs and the Lua wrapper never resumes, so `status` can keep reporting `running = true` for a task that is gone. Treat services as cancel-safe; treat `status` as advisory.

## Session Status

### crucible.set_status(opts) / crucible.clear_status(opts)

A durable, session-scoped status slot in the UI — unlike `crucible.notify`,
which is transient and easily missed. Slots are keyed, so the TUI and web
render any plugin's slots generically; the `oci` plugin uses one to show
whether a session is sandboxed.

```lua
crucible.set_status{
  session = session.id,      -- required
  key     = "oci",           -- required; one slot per key per session
  text    = "sandboxed: alpine:latest",  -- required; keep it short
  level   = "info",          -- info | warn | error (default info)
  progress = 0.4,            -- optional: fraction 0..1, or `true` for a spinner
}

crucible.clear_status{ session = session.id, key = "oci" }
```

`progress = true` means indeterminate work (render a spinner); a number is a
fraction complete, clamped to 0..1. Omit it for a state that is not work
("sandboxed: alpine"). Setting empty text is not the same as clearing —
`clear_status` removes the slot. A session's slots are dropped when it ends.

## Publications

### crucible.publish(key, value)

Publish data about the plugin itself for clients to render — not
session-scoped (that's what status slots are for). The daemon stores the
value verbatim as JSON and every client reads the same answer, keyed by
publication name and attributed to the publishing plugin.

```lua
crucible.publish("isolation", {
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

### crucible.options(tree)

Declare a settings tree that every frontend renders in its own idiom — the
settings pane in the TUI, a form on the web. The shape follows Ace3's
AceConfig options tables: nested `group` nodes whose `args` hold typed leaves,
with `get`/`set` accessors called when a value is read or written.

```lua
crucible.options{
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

`cru.paths.kiln()`, `.workspace()`, and `.session()` read the `PathsContext` the module was registered with and **raise** when that path is unconfigured (they do not return `nil`). `cru.paths.state(plugin)` is different: it ignores `PathsContext` entirely and resolves against the daemon's data root at call time — `$CRUCIBLE_HOME`, else `~/.crucible` — returning `<data root>/plugin-state/<plugin>/`, created on demand. One Lua VM serves every plugin, so a state directory baked in at registration would be the same directory for all of them; instead each plugin names itself. Two consequences:

- `paths.state("my-plugin")` works even where `paths.kiln()` raises.
- Plugin state lives under the global data root, not inside the kiln or workspace.

`plugin` must be a single path component: `""`, `"."`, `".."`, `"a/b"`, and absolute paths are refused.

## Kiln access and the `vault` name

The kiln API is `cru.kiln` / `crucible.kiln` — there is no `cru.vault` table. The old "vault" name survives in exactly one Lua-facing place: a plugin manifest may declare `capabilities: [vault]`, which parses as the `kiln` capability. (The Rust registration functions are still named `register_vault_module*`; that is internal naming only.)

## Session-VM-only: cru.defaults and cru.modes

`cru.defaults` (session default values like `system_prompt`, `temperature`) and `cru.modes` (mode definitions) are registered **only on the per-session Lua VM** — the VM that runs the shipped Lua defaults and a workspace's `.crucible/lua/init.lua`. The daemon's plugin VM never registers them, so referencing `cru.defaults` or `cru.modes` from a plugin's `init.lua` is a nil-index error. Set defaults and define modes from a workspace's `.crucible/lua/init.lua` (or a copied-out runtime defaults tree on the `runtimepath`), not from plugins.

## See Also

- [[Help/Lua/Language Basics]] -- Lua scripting overview
- [[Help/Lua/Configuration]] -- Configuration via init.lua
- [[Help/Extending/Creating Plugins]] -- Plugin development guide
- [[Help/Plugins/Oil Lua API]] -- TUI rendering primitives
