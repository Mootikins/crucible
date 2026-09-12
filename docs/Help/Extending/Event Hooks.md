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

Event hooks let you react to things happening in a Crucible session — tool calls, session startup, tool output display. Register a Lua function with `cru.on()` and it runs when the matching event fires.

## Basic Example

```lua
-- Log every tool call
cru.on("pre_tool_call", function(ctx, event)
  cru.log("info", "Tool called: " .. event.tool)
end)
```

Place this in your plugin's `init.lua` or in a `.lua` file in a loaded plugins directory. Crucible registers the handler on plugin load.

## The `cru.on()` API

```lua
-- Simple form (no options):
cru.on(event_type, handler)

-- With options:
cru.on(event_type, { pattern = "..." }, handler)

-- For one session, from inside that session:
cru.on(event_type, { session = ctx.session_id, key = "ralph" }, handler)

-- Once, and then it retires itself:
cru.on(event_type, { once = true }, handler)
```

| Argument | Type | Description |
|---|---|---|
| `event_type` | string | Event name (e.g. `"pre_tool_call"`). Must be one of the nineteen below — **exact match, no globs**. |
| `opts.pattern` | string, optional | Glob filter applied to the event's identifier (e.g. tool name). Default: match all. |
| `opts.session` | string, optional | Fire for this session alone. Must be the session the code is running in. Default: every session. See [Scoping a handler to one session](#scoping-a-handler-to-one-session). |
| `opts.key` | string, optional | What this registration calls itself, so one plugin can hold two scoped handlers on one event. |
| `opts.once` | boolean, optional | Run the handler once, then retire the registration. Default: `false`. See [Retiring a handler](#retiring-a-handler). |
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

`cru.on` is the one registration function.

## Two Registries, One Order

Handlers live in **one registry**, on the daemon VM: plugin `init.lua`s, the
shipped defaults file, and `~/.config/crucible/init.lua`. They cannot be merged — a Lua
function is only valid against the VM that created it — so dispatch runs them
in a fixed order: **session-VM handlers first, then plugin-VM handlers**,
each registry in registration order. Transforms chain across the boundary:
a plugin handler sees arguments a session handler already rewrote.

The two precognition hooks, `search:rerank` and `index:blocks` are the
exception to chaining:
they take the **first usable Transform** and stop, in registration order,
because a selection is a decision, not a patch.

## Event Types

The complete set, and it is closed: `cru.on` raises on a name that is not
here. Two Rust enums hold it — `StageId` for the thirteen turn-loop stages,
`EventName` for the ten daemon events
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
| `search:rerank` | over the merged search hits, before the cut to the caller's limit |
| `index:blocks` | over a note's block rows, before the pipeline writes them |
| `FileChanged` | a watched file was created or modified |
| `FileDeleted` | a watched file was removed |
| `FileMoved` | a watched file was renamed or moved |
| `note:created` | a note reached the index for the first time |
| `note:modified` | an already-indexed note was written again |
| `note:deleted` | a note left the index |
| `note:renamed` | a note moved, with its inbound links repointed |
| `webhook:received` | a signed delivery arrived at `POST /api/webhook/{name}` |
| `session:created` | a session was created, daemon-wide |
| `session:ended` | a session ended, daemon-wide |

The ten events below the line come off the daemon rather than an agent turn,
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

## Scoping a Handler to One Session

A handler fires for every session by default. `opts.session` narrows it to
one:

```lua
cru.on_session_start(function(session)
  if not enabled_for(session) then return end
  -- A loop that re-prompts the model when the work is not done. It must run
  -- for the sessions the user turned it on for, and for no others.
  cru.on("turn:complete", { session = session.id, key = "ralph" }, function(ctx, event)
    if not done(event) then
      return { inject = "Keep going." }
    end
  end)
end)
```

**Activation registers.** There is no list of sessions to join and nothing to
look up while a turn runs: the set of sessions a handler serves is the set of
registrations that exist.

**The host resolves the session id.** `opts.session` must name the session the
code is running in — `session.id` inside `cru.on_session_start`, or
`ctx.session_id` inside a handler. Naming any other session raises at
registration, and so does a scope from code that is in no session (a plugin
body at load, for example). A caller never writes an id it chose.

**Registering the same handler again replaces it.** The registration is keyed
by the plugin, the event, the pattern, the session and `opts.key`, so a second
call with the same five is the same registration. That matters because
`on_session_start` fires on create, on resume *and* on resume-from-storage —
and a web history fetch resumes from storage on every request. Without the
replacement each fetch would add one more handler, for ever. Give two
registrations on one event for one session two different `key` values.

**A session scope ends with the session.** Session end drops every handler
scoped to it. An unscoped handler belongs to the plugin load instead, and a
plugin reload clears those.

**A scope needs an event that carries a session.** These do not, and a
`session` scope on one raises at registration:

| Event | Why |
|---|---|
| `FileChanged`, `FileDeleted`, `FileMoved` | the file watcher fires them with no turn running |
| `note:created`, `note:modified`, `note:deleted`, `note:renamed` | the note pipeline, over a kiln's own files |
| `webhook:received` | a signed delivery from outside |
| `index:blocks` | the note pipeline again |
| `provider:auth` | the agent factory builds a chat client and holds no session |

Every other event carries one. `session:created` and `session:ended` name the
session they are about, and `search:rerank` names one when the search came
from a session.

To let an activation outlive a daemon restart, record it with
`session:set_variable(key, value)` and register again on the next
`session:start`. That is the plugin's decision — the host keeps no activation
list.

## Retiring a Handler

Two ways, and both retire the registration rather than skip it. A handler that
is skipped but still registered keeps costing a pattern match and a dispatch
for the life of the daemon.

### `{ once = true }`

The handler runs one time, and the host removes the registration. Nothing has
to be tracked on your side.

```lua
-- Warm a cache on the first tool call of the session, and never again.
cru.on("pre_tool_call", { session = session.id, once = true }, function(ctx, event)
  warm_cache()
end)
```

Three properties worth knowing:

- **The row leaves the store before the body runs.** A hook that dispatches
  the same event again from inside the handler does not re-enter it.
- **A handler that raises is still retired.** `once` counts the calls the host
  makes, not the calls that succeed.
- **A handler that never runs keeps its registration.** The permission gate
  and the provider-auth gate stop at the first hook that answers, so a `once`
  hook behind one that answered is untouched and still fires later.

`once` works on every hook, `cru.on_session_start` and
`cru.on_session_end` included.

### `cru.clear(opts?)`

Retires registrations early, and answers how many it removed.

```lua
cru.clear{ name = "pre_tool_call", session = session.id }  -- one event, one session
cru.clear{ name = "turn:complete" }                        -- one event, every scope
cru.clear{ pattern = "bash" }                              -- every row on that pattern
cru.clear()                                                -- everything this plugin registered
```

| Argument | Type | Description |
|---|---|---|
| `opts.name` | string, optional | The event to clear. A name outside the closed set raises, naming the closest match. Default: every event. |
| `opts.pattern` | string, optional | The pattern to clear, compared as **exact text**. Default: every pattern. |
| `opts.session` | string, optional | The session scope to clear. Must be the session the code is running in. Default: every scope, the unscoped rows included. |

Each option NARROWS. All of them absent clears everything the calling plugin
registered.

**A plugin clears its own registrations and cannot reach another plugin's.**
There is no owner argument, and that is deliberate: the host takes the caller
from the running plugin, so there is nothing for a caller to spell. Your own
`init.lua` is a caller like any other — it clears what it registered, and a
plugin's rows are out of its reach too.

**`opts.pattern` is exact text and is never evaluated as a glob.**
`cru.clear{ pattern = "bash" }` removes a handler registered with
`pattern = "bash"`, and leaves one registered with `pattern = "b*"` — even
though that handler fires for `bash`. Evaluating the glob here would remove
handlers you never named.

**`cru.clear` reaches the events that have their own registration function.**
`cru.on` refuses `session:start`, `session:end`, `permission:request` and
`provider:auth`, because each takes a different argument; `cru.clear` accepts
all four, so a plugin can retire what it registered through
`cru.on_session_start` and its neighbours.

Clearing is idempotent: a second call removes nothing and does not raise.

### Note lifecycle

```lua
cru.on("note:created", function(ctx, event)
  cru.log("info", "new note: " .. event.path)
end)

-- Only the daily notes:
cru.on("note:modified", { pattern = "Daily/*" }, function(ctx, event)
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
cru.on("webhook:received", { pattern = "ci" }, function(ctx, event)
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
cru.on("tool_result", { pattern = "bash" }, function(ctx, event)
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
cru.on("precognition_select", function(ctx, event)
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

`kiln` is the **name** of the `kilns` entry the note came from, never its
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

### `search:rerank`

Fires on every semantic search — the `semantic_search` tool, precognition,
`cru search` and `cru eval precognition` — after the hits from every kiln are
merged and before the list is cut to the caller's limit. A handler can rescore
the hits, reorder them, widen the span a hit names and cite further spans.
Precognition, the tool and the RPC all reach the same handlers.

```lua
cru.on("search:rerank", function(ctx, event)
  -- Prefer prose: halve the score of every heading hit.
  for _, hit in ipairs(event.hits) do
    if hit.kind == "heading" then
      hit.score = hit.score / 2
    end
  end
  table.sort(event.hits, function(a, b) return a.score > b.score end)
  return event.hits
end)
```

Event fields:
- `event.query_vector` — the query embedding, an array of numbers
- `event.kilns` — the registry names of the kilns searched
- `event.limit` — how many hits the caller asked for; the cut happens after
  the handler returns
- `event.hits` — an array of
  `{ index, kiln, path, span_start, span_end, kind, score, snippet }`,
  best first. `kiln` follows the rules of `precognition_select`. The three
  block fields are absent for a hit that names a whole note.

Return an array of entries. An entry is one of two shapes:

- `{ index, score, span_end, cited }` addresses a hit. `index` is the handle
  from `event.hits`; the others are optional. `score` replaces the hit's
  score. `span_end` widens the block a hit names and is ignored on a
  whole-note hit or when it is smaller than the hit's start. `cited` is an
  array of `{ start, stop }` pairs the hit also draws on; it reaches the tool
  result, the precognition block and `cru search` as `block.cited`.
- `{ kiln, path, span_start, score, cited }` introduces a block the kilns did
  not return. `kiln` must name one of `event.kilns`, and the block must be a
  stored row of that note, so a handler can read one with
  `cru.kiln.blocks(kiln, path)` and hand it back. The daemon reads the row
  and builds the hit from it, with the given `score` and `cited`. The first
  four fields are required.

Returned order is the order the caller sees, and the daemon cuts it to
`event.limit`; an introduced hit counts like any other.

| Return | Effect |
|--------|--------|
| `nil` | the merged order stands |
| array of entries | those hits, in that order |
| `{}` | no hits |
| anything else | warns and falls back to the merged order |

Out-of-range, duplicate and non-numeric indices are dropped with a warning.
An introduced entry is dropped with a warning when its `kiln` is not one the
search covered, when the store has no row at `span_start`, when a merged hit
already names that block, or when an earlier entry introduced it. A return
with no usable entry falls back to the merged order. The hook fails open: a
handler that errors leaves the merged order in place.

### `index:blocks`

Fires once per note, after its blocks are embedded and before their rows are
written. A handler may add synthetic rows, which the plain vector scan then
sees like any other block, and may swap the vector of a parser row. The
pipeline reaches the same handlers as every other stage.

```lua
cru.on("index:blocks", function(ctx, event)
  -- One row over each pair of adjacent embedded paragraphs.
  local extra = {}
  local prev
  for _, block in ipairs(event.blocks) do
    if block.vector then
      if prev then
        local sum = {}
        for i = 1, #block.vector do
          sum[i] = prev.vector[i] + block.vector[i]
        end
        extra[#extra + 1] = {
          span_start = prev.span_start + 1,
          span_end = block.span_end,
          kind = "transition",
          vector = cru.vec.normalize(sum),
        }
      end
      prev = block
    end
  end
  return { extra = extra }
end)
```

Event fields:
- `event.kiln` — the registry name of the kiln, absent when it has none
- `event.path` — the note's path as the index stores it
- `event.title` — the note's title; `event.description` — its `description`
  property, absent when the note has none. A handler reads them here and not
  through `cru.kiln.note`: the batch holds the kiln's connection while this
  stage fires, so a named read would wait on it until the handler budget
  stops the handler.
- `event.blocks` — an array of `{ span_start, span_end, kind, text, vector }`
  in span order. `text` is the block's own text as the parser cut it; the
  heading trail the pipeline embeds with is not on it. `vector` is absent
  for a block under the word floor.

Return `{ extra = { ... }, replace = { ... } }`. Either key may be absent.

`extra` is an array of `{ span_start, span_end, kind, vector, text }`. Extra
rows are written with the note's own rows and removed with them, so a
reprocess replaces them. `kind` must be a parser kind or `transition`.
`text` is optional; without it the row quotes the note's blocks inside its
span. A row is dropped with a warning when its span is not inside the note,
when its `span_start` is already a row's start, when its vector has a
different dimension from the note's embedded blocks, or when the note has no
embedded block to take a model name from.

`replace` is an array of `{ span_start, vector }`. Each entry swaps the
vector of the row that starts at `span_start`; the row's text and content
hash stay. The row's model name gets the suffix `#index:blocks`, so the
block cache never serves the swapped vector as the provider's: a reprocess
embeds the block again and the handler runs again over it. An entry is dropped with a warning when no
embedded row starts at `span_start` (a row under the word floor has no
vector to swap), when its vector has a different dimension from the note's
embedded blocks, or when the note has no embedded block to take a model
name from. Replacements apply before extra rows are checked.

| Return | Effect |
|--------|--------|
| `nil` | the parser's rows alone |
| `{ extra = {...} }` | the parser's rows plus the rows that pass the checks |
| `{ replace = {...} }` | the parser's rows, with the vectors that pass the checks swapped |
| anything else | warns and writes the parser's rows alone |

The hook fails open: a handler that errors leaves the parser's rows alone.

### The `retrieval-lab` plugin

`runtime/plugins/retrieval-lab/` is the worked example of both stages. One
plugin registers one `search:rerank` handler and one `index:blocks` handler,
and it composes each strategy from `cru.vec` and `cru.kiln.blocks` alone. Read
it for the shape of a real handler at either stage.

The plugin is a proof of concept, not a retrieval setting. It ships with
`enabled = false`, and it stays off until you set the two knobs:

```lua
cru.config.set({
    plugins = {
        ["retrieval-lab"] = {
            enabled = true,
            strategy = "arc_post",
        },
    },
})
```

Eight measurement runs found no strategy worth a default. At note level a
strategy moves hit@1 and MRR by about one point on a documentary corpus, and
the gains and the losses cancel. The pair and the curve strategies win at
block level only, and they cost 7 to 8 times the baseline search time. The
header of `init.luau` carries the numbers.

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
effects (writing a note, updating a statusline value), and the only event whose
`inject` return starts another turn.

The event carries what the turn knows about itself:

| Field | What it says |
|---|---|
| `event.response_length` | Length of the whole reply, in bytes |
| `event.response_tail` | The END of the reply, up to `chat.response_tail_chars` characters |
| `event.response_truncated` | `true` when the tail left text out |
| `event.stop_reason` | `end_turn`, `cancelled`, `empty`, `max_tokens` or `refusal`; `nil` when the turn ended without one |
| `event.is_continuation` | `true` on a turn an `inject` started |
| `event.continuation_depth` | How many injects precede this turn. `0` is the user's own message |
| `event.saw_tool_activity` | `true` when the turn ran a tool |

The tail is the END of the reply because that is where a model says what it
means to do next. `chat.response_tail_chars` sets the size (default 2000); `0`
sends the whole reply.

**Crucible ships no opinion about what these mean.** There is no phrase list,
no pattern and no "the model means to continue" flag: a shipped pattern would
become an API, and Crucible would then own the accuracy of a guess about a
model vendor's prose. Whether a turn finished the work is your handler's
decision, from whatever source it trusts — a plan file, a tool result, a
second model asked through `cru.session.complete`, or the text itself.

`continuation_depth` is a fact and not a limit. The host counts and does not
cap: a long plan needs as many turns as it has steps. A handler that wants a
bound reads the number and stops injecting. What stops a runaway loop either
way is the user's cancel, which reaches every depth.

## Handler Return Values

The handler's return value controls what happens next:

### Pass-through (observe only)

Return `nil` or no value. The event continues unchanged.

```lua
cru.on("pre_tool_call", function(ctx, event)
  cru.log("info", "Observing: " .. event.tool)
end)
```

### Transform

Return a table with modified fields. The event continues with the new values.

```lua
cru.on("pre_llm_call", function(ctx, event)
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
cru.on("pre_tool_call", { pattern = "*delete*" }, function(ctx, event)
  return { cancel = true, reason = "Deletes are blocked in this session" }
end)
```

### Handle (intercept execution)

Return `{ handled = true, result = ... }`. Default tool execution is skipped and your `result` becomes the tool result. Used by plugins that fully replace tool behavior — e.g. the `oci` plugin runs shell commands inside containers instead of on the host.

```lua
cru.on("pre_tool_call", { pattern = "bash" }, function(ctx, event)
  local output = run_in_container(event.args.command)
  return { handled = true, result = output }
end)
```

### Inject

Return `{ inject = { content = "..." } }` to start another turn. The content becomes the whole user message of that turn, so there is no placement option: an earlier API documented `position = "user_prefix" | "user_suffix"`, the scheduler never read it, and both values behaved identically. A handler that still sets it keeps working, and the key means nothing.

> **`turn:complete` only.** Inject is collected by the turn-completion
> dispatcher; every other event (including `pre_tool_call`) ignores it
> silently. If two handlers inject in the same turn, the last one wins.

## Lifecycle Hooks

Two named hooks for session lifecycle. These are separate from `cru.on()`.

Like `cru.on()` handlers, lifecycle hooks registered during a plugin's
load (its `init.lua` or `setup()`) belong to that plugin: reloading the plugin
clears its hooks before re-running it, so a reload never leaves a second copy
firing. Hooks registered outside a plugin load — your own `init.lua`, or the
shipped defaults file — are unowned and are never cleared by any reload.

### `cru.on_session_start(fn, opts?)`

Fires once when a session begins. Use for per-session setup (starting containers, opening connections, seeding state).

```lua
cru.on_session_start(function(session)
  cru.log("info", "Session started: " .. session.id)
end)
```

The `session` argument exposes:
- `session.id` — session id (string, read-only)
- `session.workspace` — the session's working directory, or nil (string, read-only)

Both lifecycle hooks take the same `session`, `key` and `once` options
`cru.on` does. `{ once = true }` is worth knowing here in particular, because
`on_session_start` fires again on every resume and on every resume from
storage.

By default a hook that raises is logged and the session continues. Pass
`{ required = true }` to escalate: a raising required hook **refuses the
session**. This is for hooks that establish a boundary the session must not
run without — the `oci` plugin marks its container-acquisition hook required
so a failed sandbox never silently falls back to the host.

```lua
cru.on_session_start(function(session)
  acquire_container(session)   -- raising here aborts session creation
end, { required = true })
```

The options table also takes `session` and `key`, as `cru.on` does. A start
hook scoped to one session fires when that session resumes and for no other —
see [Scoping a handler to one session](#scoping-a-handler-to-one-session).

**Where the hook runs decides what it may do.** On the plugin-VM path the
hooks are fired asynchronously, so they may call async APIs
(`cru.shell.exec`, `cru.http`, ...), and `required = true` is honoured. There
is one fire site, at session create, so those properties hold for every
`on_session_start` hook —
session refusal stays with the plugin loader, where isolation claims live.


### `cru.on_session_end(fn, opts?)`

Fires when a session ends. Use for cleanup (stopping containers, closing files).

```lua
cru.on_session_end(function(session)
  cleanup(session.id)
end)
```

The options table takes `session` and `key`, so a plugin activated for one
session can tear down for that session alone:

```lua
cru.on_session_start(function(session)
  cru.on_session_end(function(s)
    release(s.id)
  end, { session = session.id, key = "ralph" })
end)
```

There is no `required` here: the session is already over, so a failure is
logged and that is all. The end hooks run **before** the session's scoped
handlers are swept, so a scoped end hook does run.

**An `on_session_end` hook runs under the plugin that registered it.** The
executor records the owner of each hook at registration and enters that
plugin's context around the call, so `cru.storage` resolves the plugin's own
namespace there. A plugin can read at session end what one of its event
handlers stored during the session; the `reflection` plugin uses this to
read the titles precognition injected. The hook gets no intercept grant. A
hook registered outside a plugin (your `init.lua`) runs with no context.

`on_session_start` does not yet do this: it fires with no plugin context, so
`cru.storage` refuses a call from a start hook. That is a known gap, not a
rule.


## Permission Hooks

The permission layer can be driven from Lua. Register a callback that decides whether a tool call needs a prompt:

```lua
cru.permissions.on_request(function(request)
  if request.tool_name == "read_file" then
    return { allow = true }          -- auto-allow
  end
  if request.tool_name == "shell" and looks_dangerous(request.args) then
    return { deny = true }           -- auto-deny
  end
  return nil                         -- fall through to normal prompt
end)
```

> **Where to put it.** `~/.config/crucible/init.lua` and the shipped defaults
> file both run on the VM the gate dispatches, so put the callback in either.
> A plugin's `init.lua` runs on the same VM but is refused, because a hook that
> can deny a tool
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

The optional second argument takes `pattern`, `session` and
`key`. A hook scoped to one session answers for that session's turns alone —
see [Scoping a handler to one session](#scoping-a-handler-to-one-session).

## Pattern Matching

The `pattern` option uses glob syntax against the event's identifier. For `pre_tool_call`, the identifier is the tool name:

```lua
cru.on("pre_tool_call", { pattern = "*" },           fn)  -- all tools
cru.on("pre_tool_call", { pattern = "gh_*" },        fn)  -- GitHub tools
cru.on("pre_tool_call", { pattern = "just_test*" },  fn)  -- just test recipes
```

Each event decides what its identifier is; the table in **Event Types** above
lists them. For the note events it is the note path, so the same glob syntax
narrows a handler to one folder:

```lua
cru.on("note:modified", { pattern = "Daily/*" },  fn)  -- daily notes only
cru.on("webhook:received", { pattern = "ci" },    fn)  -- one webhook
```

## Handler Order

**Handlers run in the order they registered. There is no priority option.**
Neovim orders no autocommand either — `nvim_create_autocmd` takes eight
option fields and none of them ranks a handler — and a number every author
negotiates with strangers is not a coordination mechanism. Composition is
your plugin's concern, not the engine's.

Registration order is total, and three steps give it:

1. The shipped `runtime/defaults/init.luau`.
2. Your `~/.config/crucible/init.lua`.
3. The plugins, **alphabetically by plugin name** — wherever a plugin was
   found on the runtimepath. The search path decides which plugin wins a name
   clash, not when it runs.

A handler that cancels or handles the call stops the chain.

> **One consequence to know.** The shipped defaults are asked FIRST, so a
> shipped hook that decides is final. `defaults/init.luau` answers `nil` for
> every mode but `plan`, which is what leaves your own permission hooks
> reachable. In `plan` mode its deny stands and your hook cannot allow a
> mutating tool through the gate.

## Reference Plugin

The `runtime/plugins/oci/init.lua` plugin is the canonical reference for production-grade hook use. It registers one `pre_tool_call` handler per tool at load time (with `pattern`), uses `{ handled = true, result = ... }` to redirect execution into a container, and uses `on_session_start`/`on_session_end` for container lifecycle — keying its per-session state on `ctx.session_id`, since the one registration serves every session.

## Best Practices

1. **Keep handlers fast.** They run on the hot path; long operations should use `cru.timer.sleep` / `cru.timer.spawn` to yield.
2. **Use specific patterns.** A `pattern = "*"` handler runs for every tool call; narrow it if possible.
3. **Return explicitly.** If you want pass-through, `return` with no value. If you transform, return the modified event. Don't accidentally return a truthy value that Crucible interprets as a transform.
4. **Handle errors gracefully.** Check fields with `event.tool and event.tool:find(...)` rather than assuming shape.
5. **Register once.** Calls to `cru.on()` accumulate; register at plugin load, not inside another handler.

## See Also

- [[Help/Plugins/Lua Runtime API]] — full `cru.*` reference
- [[Help/Extending/Custom Handlers]] — design notes for advanced handlers
- [[Help/Extending/MCP Gateway]] — external tool integration
- [[Help/Lua/Language Basics]] — Lua syntax
