---
description: Lua scripting reference for Crucible
status: implemented
tags:
  - lua
  - luau
  - fennel
  - scripting
  - reference
---

# Lua Language Basics

Crucible uses Luau (Lua with gradual types) for plugin development, with optional Fennel support.

## Why Lua?

Lua is one of the most widely-used scripting languages, with simple syntax that's easy for both humans and LLMs to write. If you want AI to generate your plugins, Lua is an excellent choice.

## Key Features

- **Simple syntax**: Easy to learn if you know JavaScript or Python
- **Gradual types**: Optional type annotations for documentation
- **Fennel support**: Write in Lisp syntax, compile to Lua
- **LLM-friendly**: Models generate high-quality Lua code

## The `cru` Namespace

All built-in modules are accessible under the `cru` namespace (canonical). The `crucible` namespace is a backwards-compatible alias. Standalone globals like `http`, `fs`, `shell`, `oq`, and `paths` also still work.

```lua
-- Canonical access
cru.http.get(url)
cru.fs.read(path)
cru.shell("git", {"status"})
cru.log("info", "message")
cru.json.encode(tbl)
cru.json.decode(str)

-- Aliases (still work)
crucible.log("info", "message")   -- crucible.* alias
http.get(url)                     -- standalone global
```

### Core Modules

| Module | Description |
|--------|-------------|
| `cru.log(level, msg)` | Logging (`"debug"`, `"info"`, `"warn"`, `"error"`) |
| `cru.json` | `encode(table)` and `decode(string)` for JSON serialization |
| `cru.http` | HTTP client: `get`, `post`, `put`, `patch`, `delete`, `request` |
| `cru.ws` | WebSocket client: `connect(url, opts?)` returning a connection object |
| `cru.fs` | Filesystem operations |
| `cru.shell` | Shell command execution |
| `cru.oq` | Data query/transform: `parse`, `json`, `yaml`, `toml`, `toon`, `query`, `format` |
| `cru.paths` | Path utilities |
| `cru.kiln` | Kiln access |
| `cru.graph` | Knowledge graph queries |
| `cru.sessions` | Daemon session management (create, send messages, subscribe to events) |

### Utility Modules

| Module | Description |
|--------|-------------|
| `cru.timer` | `sleep(secs)`, `timeout(secs, fn)`, `clock()` |
| `cru.ratelimit` | `new({capacity, interval})` returning limiter with `:acquire()`, `:try_acquire()`, `:remaining()` |
| `cru.retry(fn, opts)` | Exponential backoff retry (opts: `max_retries`, `base_delay`, `max_delay`, `jitter`, `retryable`) |
| `cru.emitter.new()` | Event emitter with `:on(event, fn)`, `:once(event, fn)`, `:off(event, id)`, `:emit(event, ...)` |
| `cru.check` | Argument validation: `.string(val, name)`, `.number(val, name, opts)`, `.boolean(val, name)`, `.table(val, name)`, `.func(val, name)`, `.one_of(val, options, name)` -- all support `{optional=true}` |
| `cru.spawn(fn)` | Spawn an async function as an independent tokio task (daemon context only) |

## Timer

The `cru.timer` module provides async timing primitives backed by `tokio::time`.

### cru.timer.sleep(seconds)

Async sleep that yields the coroutine without blocking the runtime.

```lua
cru.timer.sleep(2.5)  -- yields for 2.5 seconds
```

The `seconds` argument must be a finite non-negative number.

### cru.timer.timeout(seconds, fn)

Run a function with a deadline. Returns `(true, result)` on success, `(false, error_string)` on error, or `(false, "timeout")` if the deadline expires.

```lua
local ok, result = cru.timer.timeout(5.0, function()
    return cru.http.get("https://slow-api.example.com")
end)
if not ok then
    cru.log("warn", "Request failed: " .. tostring(result))
end
```

### cru.timer.clock()

Returns monotonic wall-clock time in seconds (f64) since the Lua runtime started. Unlike `os.clock()` which returns CPU time, this returns wall time that advances even when the Lua VM is yielded at async points. Useful for timing and measuring elapsed durations.

```lua
local start = cru.timer.clock()
cru.timer.sleep(1.0)
local elapsed = cru.timer.clock() - start  -- ~1.0
```

## Async Task Spawning

### cru.spawn(fn)

Spawns an async Lua function as an independent tokio task (fire-and-forget). The function runs concurrently with the caller. Only available when running in daemon context with the `send` feature enabled.

This is needed when event handlers (called via `pcall`) need to perform async operations that require yielding, such as `cru.sessions.subscribe()` or `cru.sessions.send_message()`. Since `pcall`/`xpcall` create a yield barrier, spawning the async work as a separate task is the workaround.

```lua
-- Inside a gateway event handler (runs under pcall):
cru.spawn(function()
    local next_event, err = cru.sessions.subscribe(session_id)
    cru.sessions.send_message(session_id, content)
    while true do
        local event = next_event()
        if not event then break end
        -- process event
    end
end)
```

Errors in the spawned function are logged as warnings but do not propagate to the caller.

## Session API

The `cru.sessions` module provides full session management for daemon plugins. All functions are async and follow the convention of returning `(result, nil)` on success or `(nil, error_string)` on failure. Without a daemon connection, all calls return `(nil, "no daemon connected")`.

See [[Help/Plugins/Lua-Runtime-API]] for the complete reference.

### Quick example

```lua
-- Create a session
local session, err = cru.sessions.create({ type = "chat" })

-- Configure the agent
cru.sessions.configure_agent(session.id, {
    model = "claude-sonnet-4-20250514",
    system_prompt = "You are a helpful assistant.",
})

-- Subscribe to events BEFORE sending the message
local next_event, err = cru.sessions.subscribe(session.id)

-- Send a message (triggers agent processing)
local msg_id, err = cru.sessions.send_message(session.id, "Hello!")

-- Read streaming events
while true do
    local event = next_event()
    if not event then break end
    if event.type == "text_delta" then
        -- event.data.text contains the chunk
    elseif event.type == "message_complete" then
        break
    end
end

cru.sessions.unsubscribe(session.id)
cru.sessions.end_session(session.id)
```

## Fennel

Fennel is a Lisp that compiles to Lua. Use `.fnl` files if you prefer Lisp syntax with Lua's runtime.

## Resources

- [Lua Reference Manual](https://www.lua.org/manual/5.4/)
- [Luau Documentation](https://luau-lang.org/)
- [Fennel Language](https://fennel-lang.org/)
- [[Help/Concepts/Scripting Languages]] -- Language comparison
- [[Help/Extending/Creating Plugins]] -- Plugin development guide
- [[Help/Plugins/Lua-Runtime-API]] -- Complete daemon-side Lua API reference

## See Also

- [[Help/Concepts/Scripting Languages]] -- Language comparison
