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
| `event_type` | string | Event name (e.g. `"pre_tool_call"`). Must be one of the nineteen below — **exact match, no globs**. |
| `opts.pattern` | string, optional | Glob filter applied to the event's identifier (e.g. tool name). Default: match all. |
| `opts.priority` | integer, optional | Lower runs first. Default: `100`. |
| `handler` | `function(ctx, event)` | Called when the event fires and matches |

`event_type` is validated at registration. A name outside the closed set below
raises an error naming the closest match — because the dispatcher compares event
names with `==`, so a misspelt hook used to register happily and then never fire,
with only a `debug!` line to say so.

`opts.pattern` is the only glob, and it filters the event's *identifier* —
the tool name for a tool event, the note path for a note event, the webhook
name for a delivery — never the event name. An event with no identifier is
listed as such in the table below; a handler that sets `pattern` on one of
those is filtering on something that does not exist and never matches.

`cru.on` is the same function — registered on both namespaces, so
`cru.on("pre_tool_call", fn)` and `crucible.on("pre_tool_call", fn)` are
interchangeable.

## Two Registries, One Order

Handlers live in **two registries**: each session's VM (files evaluated per
session — the built-in defaults, your `init.lua`, the workspace's
`.crucible/lua/init.lua`) and the daemon's plugin VM (plugin `init.lua`s, plus
your `init.lua` evaluated after plugins load). They cannot be merged — a Lua
function is only valid against the VM that created it — so dispatch runs them
in a fixed order: **session-VM handlers first, then plugin-VM handlers**,
each registry in ascending priority. Transforms chain across the boundary:
a plugin handler sees arguments a session handler already rewrote.

The two precognition hooks are the exception to chaining:
`precognition_select` and `precognition_format` take the **first usable
Transform** and stop — session VM before plugin VM — because a selection is a
decision, not a patch.

## Event Types

The complete set, and it is closed: `crucible.on` raises on a name that is not
here. Two Rust enums hold it — `StageId` for the eleven turn-loop stages,
`EventName` for the eight daemon events
(`crucible-lua/src/handlers/hook_name.rs`) — and
`the_documented_table_lists_every_hook` fails if this table and those enums
disagree.

| Event | Fires |
|---|---|
| `pre_tool_call` | before a tool runs — can cancel, replace, or observe |
| `tool_result` | after a tool returns |
| `pre_llm_call` | before the request goes to the provider |
| `post_llm_call` | after a response has streamed |
| `transform_context` | every turn, over the assembled context |
| `precognition_select` | over candidate kiln notes, to choose which survive |
| `precognition_format` | over the surviving notes, to render the context block |
| `turn:complete` | once the whole turn has finished |
| `tool:before_execute` | immediately before execution, after permission |
| `tool:display_start` | to customise how a running tool card renders |
| `tool:display_complete` | to customise how a finished tool card renders |
| `FileChanged` | a watched file was created or modified |
| `FileDeleted` | a watched file was removed |
| `FileMoved` | a watched file was renamed or moved |
| `note:created` | a note reached the index for the first time |
| `note:modified` | an already-indexed note was written again |
| `note:deleted` | a note left the index |
| `note:renamed` | a note moved, with its inbound links repointed |
| `webhook:received` | a signed delivery arrived at `POST /api/webhook/{name}` |

The eight events below the line come off the daemon rather than an agent turn,
so they fire whether or not a session is mid-conversation — that is the point of
them. One table names them all
(`crucible-daemon/src/event_map.rs`), and both the client-facing broadcast and
this dispatch read it, so an event cannot be broadcast under one name and
hooked under another.

Their identifiers, for `opts.pattern`:

| Event | Identifier |
|---|---|
| `FileChanged`, `FileDeleted`, `FileMoved` | *none* — leave `pattern` unset |
| `note:created`, `note:modified`, `note:deleted` | the kiln-relative note path |
| `note:renamed` | the **destination** path |
| `webhook:received` | the webhook name |

Naming: the three file events are spelled in the Rust `type_name()` style
because they shipped that way and every config that registers one names them so.
Everything since is colon-namespaced.

### Note lifecycle

```lua
crucible.on("note:created", function(ctx, event)
  cru.log("info", "new note: " .. event.path)
end)

-- Only the daily notes:
crucible.on("note:modified", { pattern = "Daily/*" }, function(ctx, event)
  rebuild_digest(event.path)
end)
```

Event fields:

- `note:created` — `event.path`, `event.title`
- `note:modified` — `event.path`, `event.change_type`
- `note:deleted` — `event.path`, `event.existed` (false when the delete found nothing to remove)
- `note:renamed` — `event.from`, `event.to`

Three things to know about when they fire:

1. **They mean "this note just changed", not "this note exists".** A full kiln
   index — first open, or a forced reindex — emits none of them for the files it
   indexes. It reports one `process_complete` for the whole run instead.
   Per-file events there would put one broadcast message per note on the bus and
   say nothing new.

   The one thing a full index *does* announce is a `note:deleted` for every
   index entry whose file is gone — a `git rm` or a branch checkout while the
   daemon was down. That deletion really did happen in this run, and the
   reconciliation pass is the only place the daemon ever reports it, so a
   handler mirroring the index needs it. It is bounded by the number of stale
   entries, and zero on a kiln nothing was removed from.
2. **An unchanged file emits nothing.** Change detection skips it before the
   store is touched.
3. **A rename emits three events.** The reindex under `note.rename` really is a
   delete of the old path followed by an insert of the new one, so
   `note:deleted` and `note:created` fire as well. `note:renamed` fires last,
   once the index describes the new state, and is the event that says the two
   were one move.

### `webhook:received`

```lua
crucible.on("webhook:received", { pattern = "ci" }, function(ctx, event)
  local payload = cru.json.decode(event.body)
  cru.log("info", "CI said " .. tostring(payload.status))
end)
```

Event fields: `event.name` (the webhook name from the URL), `event.headers` (a
table; the caller's credentials and the delivery signature are stripped before
it gets here), `event.body` (the raw JSON body as a **string**, exactly as
signed — decode it yourself).

Every delivery is HMAC-verified at the HTTP edge before it reaches this hook, so
a handler never sees an unsigned one. See [[Help/Config/web]] for the secrets
file and the signature schemes — **and for the reachability caveat: the route
sits inside the web server's bearer-auth layer, which waves loopback callers
through but not remote ones.** A sender out on the internet therefore needs a
proxy or tunnel terminating on the host; pointing GitHub straight at the port
gets a 401 from the auth layer before the signature is ever checked.

### `pre_tool_call`

Fires just before a tool executes. Handlers can observe, transform, cancel, or fully handle the call.

Event fields:
- `event.type` — the event name, `"pre_tool_call"` (string)
- `event.tool` — tool name (string)
- `event.args` — tool arguments (table)

The session the call belongs to arrives as `ctx.session_id` (first handler
argument), not on the event. It is not decoration: plugin handlers are
registered once, into one Lua state shared by every session in the daemon, so
a handler holding per-session state must key it by this — see
`runtime/plugins/oci/init.lua`, which looks up the session's container with it.

Handlers receive one flat table. There is no `event.payload` envelope — it used
to leak through from the internal Rust event type, and `event.name` meant the
event type in code but the tool name in this document. Both are gone; the key
names above are pinned by `handlers::tests::conversion`.

Pattern is matched against the tool name.

### `tool_result`

Fires after a tool call finishes, over the outcome **as the model will
receive it** — including results a `pre_tool_call` handler produced via
`handled = true`, and before large outputs spill to disk. Return a partial
patch:

```lua
crucible.on("tool_result", { pattern = "bash" }, function(ctx, event)
  return { result = event.result:gsub("token=%S+", "token=[REDACTED]") }
end)
```

Event fields: `event.tool`, `event.args`, `event.result` (string),
`event.error` (string or nil). Patches chain — each handler sees the previous
one's output; `{ result = ... }` and `{ error = ... }` replace those halves,
omitted keys keep the current value. Execution already happened, so Cancel
and Handle are ignored here; a handler that must be able to veto belongs in
`pre_tool_call`. Use for redaction and summarisation of what the model sees;
`tool:display_complete` is the equivalent for what the *user* sees.

### `tool:display_start` / `tool:display_complete`

Fire around tool output display in the TUI. Use these to transform or filter how tool output is shown to the user (they don't affect the result returned to the agent).

### `tool:before_execute`

Lower-level hook fired by the in-process handler pipeline. Most plugins should use `pre_tool_call` instead — it's the canonical interception point and works uniformly across local and ACP agents.

### `precognition_select`

Fires after the kiln search, before the retrieved notes are formatted into the
system message. Handlers choose **which** notes reach the agent, in what order,
and how the snippet character budget is spent across them.

```lua
crucible.on("precognition_select", function(ctx, event)
  -- Keep only strong matches, best first. To restrict to one corpus, compare
  -- `note.kiln` — a session's kilns are a flat set with no primary.
  local picked = {}
  for _, note in ipairs(event.results) do
    if note.score > 0.7 then
      picked[#picked + 1] = { index = note.index }
    end
  end
  return picked
end)
```

Event fields:
- `event.user_message` — the query text (string)
- `event.note_count` — number of retrieved notes
- `event.char_budget` — total snippet characters the handler may allocate
- `event.results` — array of `{ index, title, score, snippet, kiln }`

`kiln` is the **name** of the `[kilns]` entry the note came from, never its
directory — a plugin is told which corpus a note is in, not where it lives on
disk. The key is **absent** when no entry claims the note's kiln, so
`if note.kiln then` answers the question it looks like it is asking; it is
never present-but-empty.

Return an array of `{ index = n, snippet = "..." }`, where `index` is the
handle from `event.results` and `snippet` optionally replaces that note's text.
Returned order is the order the agent sees. Selection is addressed **by index
rather than by value**, so the set of notes the agent sees is always a subset of
what the kiln actually returned — a handler cannot introduce a note that isn't
there.

That constrains *identity*, not *text*. `snippet` is yours to rewrite, so a
handler can still place arbitrary content under a real note's title, and this
output goes straight into the model's context. Handlers are trusted code; the
guarantee is that the note set stays real, not that its text is untouched.

| Return | Effect |
|--------|--------|
| `nil` | built-in behaviour stands |
| array of entries | those notes, in that order |
| `{}` | suppress precognition for this turn |
| anything else | warns and falls back to the built-in |

Out-of-range, duplicate and non-numeric indices are dropped with a warning
rather than failing the turn. Like every hook except `pre_tool_call`, this one
fails open: a handler that errors leaves the default in place.

The character cap still runs after your handler. It only truncates when the
total exceeds `char_budget`, so it is invisible to a handler that respects the
budget and a hard stop for one that doesn't — allocation is yours, enforcement
stays with the daemon.

> **Snippets are measured in characters, not bytes.** Lua string indexing is
> byte-based, so `snippet:sub(1, n)` disagrees with the budget on any non-ASCII
> text and can slice a UTF-8 sequence in half. Use `utf8.offset`:
>
> ```lua
> local stop = utf8.offset(snippet, n + 1)
> snippet = stop and snippet:sub(1, stop - 1) or snippet
> ```

Use `precognition_format` (below) to change how the chosen notes are *rendered*;
use this to change *which* ones there are.

### `precognition_format`

Fires after selection, over the notes that survived it. Return a string to
replace the entire system-message body that carries the kiln context.

Event fields: `event.user_message`, `event.note_count`, and `event.results`
(array of `{ title, score, snippet, kiln }`). `kiln` is a registry name and
follows the same rules as in `precognition_select` above — no `index`, because
these notes have already been chosen and there is nothing to address them by.

Does **not** fire when the search returned nothing — the daemon short-circuits
before invoking it. To inject something on the empty case, use
`transform_context`, which fires every turn.

### `pre_llm_call` / `post_llm_call`

`pre_llm_call` fires once per provider request, before it is sent.
`post_llm_call` fires after the response has finished streaming and carries
`event.response_summary`, `event.model` and `event.duration_ms`.

### `transform_context`

Fires every turn, over the assembled context, whether or not a kiln search ran.
The hook for "always add something", where `precognition_format` is the hook for
"reshape what the search found".

### `turn:complete`

Fires once when the whole turn has finished — after the final
`message_complete`, not once per LLM call. The place for end-of-turn side
effects (writing a note, updating a statusline value).

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

> **For `pre_tool_call`, return `{ args = { ... } }`** to rewrite the call's
> arguments before execution — path remapping, flag injection, sanitisation.
> Rewrites chain: later handlers see the rewritten value, and the executor's
> own typed argument parsing validates the result exactly as it validates
> model-supplied arguments. The rewrite must be *returned*; mutating
> `event.args` in place does nothing (the event table is a projection, not
> the call).

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

> **`turn:complete` only.** Inject is collected by the turn-completion
> dispatcher; every other event (including `pre_tool_call`) ignores it
> silently. If two handlers inject in the same turn, the last one wins.

## Lifecycle Hooks

Two named hooks for session lifecycle. These are separate from `crucible.on()`.

Like `crucible.on()` handlers, lifecycle hooks registered during a plugin's
load (its `init.lua` or `setup()`) belong to that plugin: reloading the plugin
clears its hooks before re-running it, so a reload never leaves a second copy
firing. Hooks registered outside a plugin load — your own `init.lua`, or a
session VM — are unowned and are never cleared by any plugin's reload.

### `crucible.on_session_start(fn, opts?)`

Fires once when a session begins. Use for per-session setup (starting containers, opening connections, seeding state).

```lua
crucible.on_session_start(function(session)
  cru.log("info", "Session started: " .. session.id)
end)
```

The `session` argument exposes:
- `session.id` — session id (string, read-only)
- `session.workspace` — the session's working directory, or nil (string, read-only)

By default a hook that raises is logged and the session continues. Pass
`{ required = true }` to escalate: a raising required hook **refuses the
session**. This is for hooks that establish a boundary the session must not
run without — the `oci` plugin marks its container-acquisition hook required
so a failed sandbox never silently falls back to the host.

```lua
crucible.on_session_start(function(session)
  acquire_container(session)   -- raising here aborts session creation
end, { required = true })
```

**Where the hook runs decides what it may do.** On the plugin-VM path the
hooks are fired asynchronously, so they may call async APIs
(`cru.shell.exec`, `cru.http`, ...), and `required = true` is honoured. Hooks
registered on the **session VM** (your `init.lua` or the workspace's
`.crucible/lua/init.lua` deciding a session's opening configuration) run
**synchronously** during session-VM construction: they cannot await async
APIs, they fail open per hook, and `required` is not honoured there —
session refusal stays with the plugin loader, where isolation claims live.


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

> **Session-scoped Lua only.** This API exists on each session's VM — put the
> callback in your workspace's `.crucible/lua/init.lua`. It is *not*
> registered on the plugin runtime, so a plugin's `init.lua` cannot use it
> yet; a plugin wanting to gate tools should use `pre_tool_call` with
> `cancel` instead.

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

Each event decides what its identifier is; the table in **Event Types** above
lists them. For the note events it is the note path, so the same glob syntax
narrows a handler to one folder:

```lua
crucible.on("note:modified", { pattern = "Daily/*" },  fn)  -- daily notes only
crucible.on("webhook:received", { pattern = "ci" },    fn)  -- one webhook
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

The `runtime/plugins/oci/init.lua` plugin is the canonical reference for production-grade hook use. It registers one `pre_tool_call` handler per tool at load time (with `pattern` and priority 10), uses `{ handled = true, result = ... }` to redirect execution into a container, and uses `on_session_start`/`on_session_end` for container lifecycle — keying its per-session state on `ctx.session_id`, since the one registration serves every session.

## Best Practices

1. **Keep handlers fast.** They run on the hot path; long operations should use `cru.timer.sleep` / `cru.spawn` to yield.
2. **Use specific patterns.** A `pattern = "*"` handler runs for every tool call; narrow it if possible.
3. **Return explicitly.** If you want pass-through, `return` with no value. If you transform, return the modified event. Don't accidentally return a truthy value that Crucible interprets as a transform.
4. **Handle errors gracefully.** Check fields with `event.tool and event.tool:find(...)` rather than assuming shape.
5. **Register once.** Calls to `crucible.on()` accumulate; register at plugin load, not inside another handler.

## See Also

- [[Help/Plugins/Lua Runtime API]] — full `cru.*` reference
- [[Help/Extending/Custom Handlers]] — design notes for advanced handlers
- [[Help/Extending/MCP Gateway]] — external tool integration
- [[Help/Lua/Language Basics]] — Lua syntax
