---
title: Ask Module
description: Reference for the Lua ask module integration points.
tags: [help, lua, api]
---

# Ask Module

`ask` is a **global** table (not under `cru.`) registered into every Crucible Lua state. It
builds structured question batches and the answer values that pair with them.

> **Scope.** The module is a *constructor* API. It builds `AskQuestion`, `AskBatch`,
> `QuestionAnswer`, and `AskBatchResponse` values and can log a notification. Submitting a
> batch to a user and awaiting the reply happens on the Rust side (`LuaAskContext::ask_user`
> in `crates/crucible-lua/src/ask/context.rs`); there is currently no Lua binding that sends
> a batch and blocks for the response. Build batches, hand them to a host that knows how to
> present them, or use the constructors to fabricate answers in tests.

## Module functions

| Function | Returns | Notes |
|----------|---------|-------|
| `ask.question(header, text)` | `AskQuestion` | Both arguments are required strings |
| `ask.batch()` | `AskBatch` | Empty batch with a fresh UUID |
| `ask.answer(indices)` | `QuestionAnswer` | Array of selected choice indices |
| `ask.answer_other(text)` | `QuestionAnswer` | A free-text ("other") answer |
| `ask.notify(message)` | – | Logs at `info` on the `lua_notify` target |

## `AskQuestion`

Builder methods return a **new** question rather than mutating in place, so they chain:

```lua
local q = ask.question("Library", "Which async runtime?")
    :choice("Tokio")
    :choice("async-std")

local features = ask.question("Features", "Select features")
    :choices({ "Auth", "Logging", "Caching" })
    :multi_select()
```

| Method | Returns | Description |
|--------|---------|-------------|
| `:choice(label)` | `AskQuestion` | Append one choice |
| `:choices({...})` | `AskQuestion` | Append several choices |
| `:multi_select([enabled])` | `AskQuestion` | Enable multi-select; the argument defaults to `true` |
| `:header()` | string | The header passed to `ask.question` |
| `:question_text()` | string | The question text |
| `:get_choices()` | table | Choice labels |
| `:is_multi_select()` | boolean | Whether multi-select is on |

Because the builders are non-mutating, `q:choice("X")` alone does nothing — you must keep
the returned value.

## `AskBatch`

```lua
local batch = ask.batch()
    :question(q)
    :question(features)

print(batch:question_count())  -- 2
print(batch:id())              -- UUID string
```

| Method | Returns | Description |
|--------|---------|-------------|
| `:question(q)` | `AskBatch` | Append a question (chainable, non-mutating) |
| `:id()` | string | The batch's UUID |
| `:question_count()` | number | How many questions the batch holds |

## `QuestionAnswer`

```lua
local picked = ask.answer({ 0, 2 })      -- choice indices are 0-based
local custom = ask.answer_other("Redis") -- free-text instead of a choice
```

| Method | Returns | Description |
|--------|---------|-------------|
| `:selected()` | table | Selected choice indices (0-based) |
| `:first_selected()` | number \| nil | First selected index, or nil when nothing is selected |
| `:has_selection()` | boolean | Whether any choice was selected |
| `:selection_count()` | number | Number of selected choices |
| `:other()` | string \| nil | The free-text answer, if any |
| `:has_other()` | boolean | Whether a free-text answer was given |

## `AskBatchResponse`

Produced by the host when a batch is answered.

| Method | Returns | Description |
|--------|---------|-------------|
| `:id()` | string | UUID of the batch this answers |
| `:is_cancelled()` | boolean | True when the user dismissed the batch |
| `:answer_count()` | number | Number of answers carried |
| `:answer(i)` | `QuestionAnswer` | **1-based** index; errors if out of bounds |
| `:answers()` | table | All answers, as a 1-based array |
| `:has_answers()` | boolean | Whether any answers are present |

Note the index bases differ: `:answer(i)` takes a 1-based index (Lua convention) while
`:selected()` returns 0-based *choice* indices (the wire format).

```lua
local function process(response)
    if response:is_cancelled() then
        return
    end
    for _, answer in ipairs(response:answers()) do
        if answer:has_other() then
            print("Custom: " .. answer:other())
        else
            print("Selected: " .. tostring(answer:first_selected()))
        end
    end
end
```

## Fennel

The same API, with method calls via `:`:

```fennel
(local q (-> (ask.question "Library" "Which async runtime?")
             (: :choice "Tokio")
             (: :choice "async-std")))

(local batch (-> (ask.batch) (: :question q)))
```

## Related: `cru.interaction`

`cru.interaction.ask{...}` is a different, table-shaped constructor that produces a plain
request table (`question`, `choices`, `multi_select`, `allow_other`) rather than the
userdata builders above. It sits alongside `cru.interaction.popup`, `.panel`, and
`.permission`. Like `ask`, it constructs a request; it does not present one.

## See Also

- [[Help/Plugins/Lua Runtime API]] — the full `cru.*` surface
- [[Help/Extending/Creating Plugins]] — plugin structure and lifecycle
