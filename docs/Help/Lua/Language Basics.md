---
title: "Luau Language Basics"
description: Luau scripting reference for Crucible
status: implemented
tags:
  - lua
  - luau
  - scripting
  - reference
---

# Luau Language Basics

Crucible embeds Luau (via the `mlua` crate) for plugin development. Luau is Lua
with a gradual type system: a plugin is ordinary Lua until it declares types.

**Most of the benefit needs no annotations at all.** Crucible generates a
declarations file for every `cru.*` function, and the checker reads it whether
or not your file says `--!strict`. A misspelled namespace, a wrong argument
count and a wrong argument type are caught in plain Lua. `--!strict` adds
checks on your OWN code.

## Why Luau?

Lua is one of the most widely-used scripting languages, with simple syntax that's easy for both humans and LLMs to write. If you want AI to generate your plugins, Lua is an excellent choice. Luau adds the type annotations that make a generated plugin checkable before it runs.

## Key Features

- **Simple syntax**: Easy to learn if you know JavaScript or Python
- **Gradual types**: annotate what matters, leave the rest untyped
- **LLM-friendly**: Models generate high-quality Lua code

## What Luau does not have

Luau is not PUC Lua 5.4, and three differences reach plugin authors:

| Missing | Use instead |
|---------|-------------|
| `package.path`, `package.searchers` | Nothing. The host resolves `require` over the plugin roots. `package.loaded` and `package.preload` work. |
| `setfenv`, `getfenv`, `loadstring` | `load`, and ordinary upvalues. |
| `goto` labels | A loop or an early return. |

Crucible provides `io` and the file half of `os` (`getenv`, `tmpname`,
`remove`, `rename`) itself, so a plugin reads and writes files exactly as it
did under PUC Lua. There is deliberately no `io.popen` and no `os.execute`:
running a command is `cru.shell`'s job, which the permission layer gates.

## The `cru` Namespace

All built-in modules live under the `cru` namespace — the one Lua global.
There are no standalone globals: `http`, `fs`, `shell`, `paths` and `graph`
were removed, and referencing one is a nil-index error naming the field.

Crucible adds what Lua lacks and nothing more. Reading and writing files is
`io`'s job, joining strings is the language's, and formatting is
`string.format` — so `cru.fs.read`/`write`/`append`/`rename`, `cru.paths.join`
and `cru.fmt` are gone.

```lua
-- Canonical access
cru.http.get(url)
cru.shell.exec("git", {"status"})
cru.log("info", "message")
cru.json.encode(tbl)
cru.json.decode(str)

-- Files are plain Lua
local f = assert(io.open(path, "r"))
local body = f:read("a")
f:close()
```

> [!warning] One known divergence: `config`
> `cru.config.get(key)` reads a single **top-level** app-config value (the
> merged `config.toml` + `cru.config.set()` state, no dotted paths), while
> `cru.plugin.config.get("plugin.key")` — registered on the daemon's plugin VM —
> does dotted-key descent into `[plugins.*]` config. Same name, different
> semantics; pick by what you're reading, not by namespace habit.

### Core Modules

| Module | Description |
|--------|-------------|
| `cru.log(level, msg)` | Logging (`"debug"`, `"info"`, `"warn"`, `"error"`) |
| `cru.json` | `encode(table)`, `decode(string)`, and `array(table)` (mark a table as a JSON list so an empty one encodes as `[]`, not `{}`) |
| `cru.http` | HTTP client: `get`, `post`, `put`, `patch`, `delete`, `request` |
| `cru.ws` | WebSocket client: `connect(url, opts?)` returning a connection object |
| `cru.fs` | The filesystem gap Lua's `io` does not cover: `exists`, `is_file`, `is_dir`, `list`, `mkdir`, `copy`, `remove_all`. Read and write with `io`. |
| `cru.shell` | Shell command execution |
| `cru.oq` | Data query/transform: `parse`, `yaml`, `toml`, `toon`, `query`, `format` (JSON is `cru.json`) |
| `cru.paths` | Directories the host owns: `config`, `workspace`, `session`, `state(plugin)`. Join with `..`. |
| `cru.kiln` | Kiln access |
| `cru.session` | Daemon session management (create, send messages, subscribe to events) |

### Kiln-Addressed Paths

A plugin addresses a kiln by NAME and asks the daemon to resolve it:
`cru.kiln.path(name, relative?)`. The name comes from something the plugin
already knows — a session's `kilns` array, or `cru.kiln.active`.

The `kiln://` URL scheme is removed. It never said WHICH kiln registry to
consult and it collided with plain relative paths, so one function replaced
it. Every surviving `cru.fs` function refuses a `kiln://` prefix with an
error naming the replacement, rather than treating it as a relative path and
silently creating a `./kiln:/...` directory.

```lua
local root = cru.kiln.path("notes")
local dir  = cru.kiln.path("notes", ".crucible/proposals")
cru.fs.mkdir(dir)

local f = assert(io.open(dir .. "/idea.md", "w"))
f:write(body)
f:close()
```

The relative part must be plain components — `..`, `.` and absolute parts
are refused. That is a bug lint, not a boundary: a plugin builds the
relative half from pieces it already knows, so a `..` there is a mistake
worth reporting.


### Plugin & Agent Modules

| Module | Description |
|--------|-------------|
| `cru.storage` | Plugin-scoped key-value store: `set(entity, key, val)`, `get(entity, key)`, `list(entity)`, `find(key, val)`, `delete(entity, key)` |
| `cru.schedule` | Interval tasks: `cru.schedule({every=N}, fn)` returns handle; `cru.schedule.cancel(handle)` |
| `cru.tools` | Tool registry: `get_tools()`, `run(name, args)` |
| `cru.log.notify` | Notifications: `notify(msg, level?, opts?)`, `notify_once(msg)` |
| `cru.log.messages` | Notification panel: `toggle()`, `show()`, `hide()`, `clear()` |
| `cru.oil` | UI building: `text()`, `col()`, `row()`, `spacer()`, `maybe()`, `match_state()` |
| `cru.errors` | Plugin error log: `recent(n?)` returns recent errors |

### Utility Modules

| Module | Description |
|--------|-------------|
| `cru.timer` | `sleep(secs)`, `timeout(secs, fn)`, `clock()` |
| `cru.ratelimit` | `new({capacity, interval})` returning limiter with `:acquire()`, `:try_acquire()`, `:remaining()` |
| `cru.retry(fn, opts)` | Exponential backoff retry (opts: `max_retries`, `base_delay`, `max_delay`, `jitter`, `retryable`) |
| `cru.emitter.new()` | Event emitter with `:on(event, fn)`, `:once(event, fn)`, `:off(event, id)`, `:emit(event, ...)` |
| `cru.check` | Argument validation: `.string(val, name)`, `.number(val, name, opts)`, `.boolean(val, name)`, `.table(val, name)`, `.func(val, name)`, `.one_of(val, options, name)` -- all support `{optional=true}` |
| `cru.timer.spawn(fn)` | Spawn an async function as an independent tokio task (daemon context only) |
| `cru.inspect(value, opts?)` | Pretty-print any value with cycle detection (`<cycle: table>`); opts: `max_depth`, `indent`. Also available as the global `inspect` |
| `cru.tbl_deep_extend(behavior, ...)` | Deep-merge tables into a new table; `behavior` is `"force"` (last wins) or `"keep"` (first wins) |
| `cru.tbl_get(t, ...)` | Safe nested access: `cru.tbl_get(cfg, "a", "b", "c")` returns the value or `nil` if any step is missing or not a table |
| `cru.on_error` | Reserved error-handler slot, initialized to `nil`. Assignable, but nothing invokes it yet |

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

### cru.timer.spawn(fn)

Spawns an async Lua function as an independent tokio task (fire-and-forget). The function runs concurrently with the caller. Only available when running in daemon context with the `send` feature enabled.

This is needed when event handlers (called via `pcall`) need to perform async operations that require yielding, such as `cru.session.subscribe()` or `cru.session.send_message()`. Since `pcall`/`xpcall` create a yield barrier, spawning the async work as a separate task is the workaround.

```lua
-- Inside a gateway event handler (runs under pcall):
cru.timer.spawn(function()
    local next_event, err = cru.session.subscribe(session_id)
    cru.session.send_message(session_id, content)
    while true do
        local event = next_event()
        if not event then break end
        -- process event
    end
end)
```

Errors in the spawned function are logged as warnings but do not propagate to the caller.

## Session API

The `cru.session` module provides full session management for daemon plugins. All functions are async and follow the convention of returning `(result, nil)` on success or `(nil, error_string)` on failure. Without a daemon connection, all calls return `(nil, "no daemon connected")`.

See [[Help/Plugins/Lua Runtime API]] for the complete reference.

### Quick example

```lua
-- Create a session
local session, err = cru.session.create({ type = "chat" })

-- Configure the agent
cru.session.configure_agent(session.id, {
    model = "claude-sonnet-4-20250514",
    system_prompt = "You are a helpful assistant.",
})

-- Subscribe to events BEFORE sending the message
local next_event, err = cru.session.subscribe(session.id)

-- Send a message (triggers agent processing)
local msg_id, err = cru.session.send_message(session.id, "Hello!")

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

cru.session.unsubscribe(session.id)
cru.session.end_session(session.id)
```

## Types

```lua
--!strict
type Task = { text: string, done: boolean }

local function render(task: Task): string
    return (task.done and "[x] " or "[ ] ") .. task.text
end
```

The annotations are erased at runtime. `luau-lsp analyze` is what checks them;
see [[Help/Extending/Creating Plugins]] for the plugin scaffold.

### Five idioms `--!strict` asks for

Every plugin Crucible ships is `--!strict`, and getting there needed the same
five changes over and over. None of them is a workaround: each one is correct
Lua that the checker cannot see through, and writing it the other way says out
loud what a reader had to infer.

**1. Annotate the local, not the expression.**

```lua
-- Reads as a defect: `args or {}` widens to `Args | {}`, and a field read
-- fails against the empty half.
args = args or {}

-- Correct, and the type is stated once.
local args: Args = options or {}
```

**2. `pcall` on a function that answers with nothing.**

Luau types `pcall` as `(boolean, R...)`. With an empty `R...` there is no
second slot, even though at run time the error is always there.

```lua
local ok, err = pcall(function()
    do_the_thing()
    return nil
end)
```

**3. Parenthesise a multi-return call used as the last argument.**

`string.find` answers `(start, stop)`. As the last argument it silently fills
the NEXT parameter — eighteen assertions in the shipped suites were passing a
match position as their failure message.

```lua
expect.truthy((text:find("needle", 1, true)))
```

**4. `assert`, not a truthiness assertion, when the next line indexes.**

Both fail when the value is missing. Only `assert` narrows the type.

```lua
local handle = assert(io.open(path, "r"))
```

**5. Give every exit the same arity.**

A function answering five values on one path and one `nil` on another leaves
four names unbound at the call site. Say `return nil, nil, nil, nil, nil`.

A pattern like `("%s*(.-)%s*"):match` always succeeds, but no checker knows
that. Write `(s:match(...)) or s` rather than asserting something a reader has
to verify.

## Resources

- [Luau Reference](https://luau.org/)
- [Lua 5.1 Reference Manual](https://www.lua.org/manual/5.1/) — the language Luau derives from
- [[Help/Concepts/Scripting Languages]] -- the scripting reference
- [[Help/Extending/Creating Plugins]] -- Plugin development guide
- [[Help/Plugins/Lua Runtime API]] -- Complete daemon-side Lua API reference

## See Also

- [[Help/Concepts/Scripting Languages]] -- Language comparison
