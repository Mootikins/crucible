---
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

This page documents the `cru.*` Lua API available to plugins running inside the Crucible daemon (`cru-server`). All modules are registered under both the `cru` and `crucible` namespaces. Some modules (like `http`, `oq`, `fs`) are also available as standalone globals for backwards compatibility.

For TUI-specific Lua APIs (Oil rendering primitives), see [[Help/Plugins/Oil-Lua-API]].

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

Create a new session. Returns a session table with at least `{ id, session_type, state, kiln, workspace }`.

```lua
local session, err = cru.sessions.create({
    type = "chat",                            -- session type (default: "chat")
    kiln = "/path/to/notes",                  -- kiln path (default: crucible home)
    workspace = "/path/to/workspace",         -- workspace path (optional)
    kilns = { "/extra/notes", "/more/docs" }, -- connected kilns for knowledge (optional)
})
```

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

```lua
cru.sessions.interaction_respond(session_id, request_id, { approved = true })
```

### Full subscribe/respond pattern

This is the pattern used by the Discord plugin's responder module. Subscribe *before* sending the message to avoid missing early events:

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

## See Also

- [[Help/Lua/Language Basics]] -- Lua scripting overview
- [[Help/Lua/Configuration]] -- Configuration via init.lua
- [[Help/Extending/Creating Plugins]] -- Plugin development guide
- [[Help/Plugins/Oil-Lua-API]] -- TUI rendering primitives
