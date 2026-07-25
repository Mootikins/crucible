---
title: Event Hooks
description: React to events in your kiln with Lua scripts
status: implemented
tags:
  - extending
  - hooks
  - lua
  - events
aliases:
  - Hooks
  - Lua Hooks
---

# Event Hooks

Event hooks let you react to things happening in a Crucible session — tool calls, session startup, tool output display. Register a Lua function with `crucible.on()` and it runs when the matching event fires.

## Basic Example

```lua
-- Log every tool call
crucible.on("pre_tool_call", function(ctx, event)
  cru.log("info", "Tool called: " .. event.tool)
end)
```

Place this in your plugin's `init.lua` or in a `.lua` file in a loaded plugins directory. Crucible registers the handler on plugin load.

## The `crucible.on()` API

```lua
-- Simple form (no options):
crucible.on(event_type, handler)

-- With options:
crucible.on(event_type, { pattern = "...", priority = 50 }, handler)
```

| Argument | Type | Description |
|---|---|---|
| `event_type` | string | Event name (e.g. `"pre_tool_call"`) |
| `opts.pattern` | string, optional | Glob filter applied to the event's identifier (e.g. tool name). Default: match all. |
| `opts.priority` | integer, optional | Lower runs first. Default: `100`. |
| `handler` | `function(ctx, event)` | Called when the event fires and matches |

## Event Types

### `pre_tool_call`

Fires just before a tool executes. Handlers can observe, transform, cancel, or fully handle the call.

Event fields:
- `event.type` — the event name, `"pre_tool_call"` (string)
- `event.tool` — tool name (string)
- `event.args` — tool arguments (table)
- `event.session_id` — the session the call belongs to (string)

`event.session_id` is not decoration. Plugin handlers are registered once, into
one Lua state shared by every session in the daemon, so a handler holding
per-session state must key it by this — see `runtime/plugins/oci/init.lua`,
which looks up the session's container with it.

Handlers receive one flat table. There is no `event.payload` envelope — it used
to leak through from the internal Rust event type, and `event.name` meant the
event type in code but the tool name in this document. Both are gone; the key
names above are pinned by `handlers::tests::conversion`.

Pattern is matched against the tool name.

### `tool:display_start` / `tool:display_complete`

Fire around tool output display in the TUI. Use these to transform or filter how tool output is shown to the user (they don't affect the result returned to the agent).

### `tool:before_execute`

Lower-level hook fired by the in-process handler pipeline. Most plugins should use `pre_tool_call` instead — it's the canonical interception point and works uniformly across local and ACP agents.

## Handler Return Values

The handler's return value controls what happens next:

### Pass-through (observe only)

Return `nil` or no value. The event continues unchanged.

```lua
crucible.on("pre_tool_call", function(ctx, event)
  cru.log("info", "Observing: " .. event.tool)
end)
```

### Transform

Return a table with modified fields. The event continues with the new values.

```lua
crucible.on("pre_llm_call", function(ctx, event)
  return { prompt = event.prompt .. " (be concise)" }
end)
```

> **Not supported for `pre_tool_call`.** That dispatcher honours only Cancel and
> Handle; a returned table is ignored. Mutating `event.args` and returning it
> looks like it sanitises the call but the original arguments still execute — use
> **Handle** to run a sanitised version yourself, or **Cancel** to block it.

### Cancel

Return `{ cancel = true, reason = "why" }`. The tool call is aborted and the reason surfaces to the agent as an error.

```lua
crucible.on("pre_tool_call", { pattern = "*delete*", priority = 5 }, function(ctx, event)
  return { cancel = true, reason = "Deletes are blocked in this session" }
end)
```

### Handle (intercept execution)

Return `{ handled = true, result = ... }`. Default tool execution is skipped and your `result` becomes the tool result. Used by plugins that fully replace tool behavior — e.g. the `oci` plugin runs shell commands inside containers instead of on the host.

```lua
crucible.on("pre_tool_call", { pattern = "bash", priority = 10 }, function(ctx, event)
  local output = run_in_container(event.args.command)
  return { handled = true, result = output }
end)
```

### Inject

Return `{ inject = { content = "...", position = "user_prefix" } }` to prepend/append content to the user's next prompt. `position` can be `"user_prefix"` or `"user_suffix"`.

## Lifecycle Hooks

Three named hooks for session lifecycle. These are separate from `crucible.on()` and take a single function.

### `crucible.on_session_start(fn)`

Fires once when a session begins. Use for per-session setup (starting containers, opening connections, seeding state).

```lua
crucible.on_session_start(function(session)
  cru.log("info", "Session started: " .. session.id)
end)
```

The `session` argument exposes:
- `session.id` — session id (string, read-only)
- `session.workspace` — the session's working directory, or nil (string, read-only)

Handlers may call async APIs (`cru.shell.exec`, `cru.http`, ...) — lifecycle
hooks run inside the daemon's async runtime.

### `crucible.on_tools_registered(fn)`

Fires when tools from an MCP server or backend become available.

```lua
crucible.on_tools_registered(function(evt)
  cru.log("info", "Tools from " .. evt.server_name .. ":")
  for _, tool in ipairs(evt.tools) do
    cru.log("info", "  - " .. tool.name)
  end
end)
```

Event fields:
- `evt.server_name` — name of the MCP/tool source
- `evt.tools` — array of `{ name, description, display_name }`

### `crucible.on_session_end(fn)`

Fires when a session ends. Use for cleanup (stopping containers, closing files).

```lua
crucible.on_session_end(function(session)
  cleanup(session.id)
end)
```

## Permission Hooks

The permission layer can be driven from Lua. Register a callback that decides whether a tool call needs a prompt:

```lua
crucible.permissions.on_request(function(request)
  if request.tool_name == "read_file" then
    return { allow = true }          -- auto-allow
  end
  if request.tool_name == "shell" and looks_dangerous(request.args) then
    return { deny = true }           -- auto-deny
  end
  return nil                         -- fall through to normal prompt
end)
```

Request fields:
- `request.tool_name` — tool being requested
- `request.args` — tool arguments
- `request.file_path` — path (if the tool touches a file)

Return:
- `{ allow = true }` — grant without prompting
- `{ deny = true }` — deny without prompting
- `nil` — show the normal permission prompt

## Pattern Matching

The `pattern` option uses glob syntax against the event's identifier. For `pre_tool_call`, the identifier is the tool name:

```lua
crucible.on("pre_tool_call", { pattern = "*" },           fn)  -- all tools
crucible.on("pre_tool_call", { pattern = "gh_*" },        fn)  -- GitHub tools
crucible.on("pre_tool_call", { pattern = "just_test*" },  fn)  -- just test recipes
```

## Priority Guide

Lower numbers run earlier:

| Range | Use |
|-------|-----|
| 0–9 | Security / validation / cancels |
| 10–49 | Interception (container runtimes, sandboxing) |
| 50–99 | Transformation |
| 100–149 | General observation (default) |
| 150–199 | Logging / audit |

When multiple handlers fire for the same event, they run in ascending priority order. A handler that cancels or handles the call stops the chain.

## Reference Plugin

The `runtime/plugins/oci/init.lua` plugin is the canonical reference for production-grade hook use. It registers one `pre_tool_call` handler per tool at load time (with `pattern` and priority 10), uses `{ handled = true, result = ... }` to redirect execution into a container, and uses `on_session_start`/`on_session_end` for container lifecycle — keying its per-session state on `event.session_id`, since the one registration serves every session.

## Best Practices

1. **Keep handlers fast.** They run on the hot path; long operations should use `cru.timer.sleep` / `cru.spawn` to yield.
2. **Use specific patterns.** A `pattern = "*"` handler runs for every tool call; narrow it if possible.
3. **Return explicitly.** If you want pass-through, `return` with no value. If you transform, return the modified event. Don't accidentally return a truthy value that Crucible interprets as a transform.
4. **Handle errors gracefully.** Check fields with `event.tool and event.tool:find(...)` rather than assuming shape.
5. **Register once.** Calls to `crucible.on()` accumulate; register at plugin load, not inside another handler.

## See Also

- [[Help/Plugins/Lua-Runtime-API]] — full `cru.*` reference
- [[Help/Extending/Custom Handlers]] — design notes for advanced handlers
- [[Help/Extending/MCP Gateway]] — external tool integration
- [[Help/Lua/Language Basics]] — Lua syntax
