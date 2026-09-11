---
title: Session Compaction
description: The compaction RPCs and autocompact threshold config key — and why triggering them currently wedges a session
status: partial
tags:
  - sessions
  - context
  - reference
---

# Session Compaction

Session compaction is meant to shrink a long conversation once it approaches the
session's context budget. The knobs and the RPC surface exist; the compaction
itself does not.

> [!warning] Implementation status
> **Nothing is ever compacted.** `session.compact` — and the automatic trigger —
> set the session's state to `compacting` and stop there. No code consumes that
> state (it appears only in state display and `session.list` filtering), so no
> summarizing or trimming happens and nothing transitions the session back out
> on its own. Chatting still works — the send path has no lifecycle-state
> guard — but the state is stuck: `session.pause` and a second `session.compact`
> both require state `active`, `session.resume` requires `paused`, so all three
> fail with an invalid-state error. Two exits exist: `session.end` (accepts any
> non-ended state) and `session.resume_from_storage`, which revives a session
> in any persisted state — including `compacting` — back to `active`. The
> `{"compaction_requested": true}` reply from `session.compact` is misleading —
> the request changes the state string and nothing else.
>
> Because the trigger is automatic, setting a `context_budget` and chatting past
> the threshold sticks the session in `compacting` with no further user action.
> Until compaction is implemented: if you set a `context_budget`, also set
> `chat.autocompact_threshold` to `0` — `:set context_strategy` truncation
> still enforces the budget without the trigger. Leaving
> `context_budget` unset avoids the trigger too, but strategy-based budget
> enforcement is keyed to the same budget, so it disables that as well.

## The threshold

`chat.autocompact_threshold` is a config key: a fraction of `context_budget`,
one value per install rather than per session.

- unset — uses the default, **0.95**
- `<= 0.0` — explicitly disabled
- `>= 1.0` — fires only when usage strictly exceeds the full budget
- no `context_budget` set — never fires. `context_budget` is unset by default,
  so auto-compaction is opt-in.

The check runs after each completed turn, when the provider reports token
usage: if `prompt_tokens` strictly exceeds `context_budget * threshold`, the
daemon requests compaction. At the default threshold with a budget of 1000,
usage of 950 does not trigger; 951 does. Once the session is in `compacting`
state, later triggers fail their state guard silently (logged at debug level).

## Setting it

It is a config key, so it is written the way every other key is — from
`~/.config/crucible/init.lua`, from the settings pane, or with `:set`:

```lua
cru.config.set { chat = { autocompact_threshold = 0.8 } }
```

```
:set chat.autocompact_threshold=0.8
:set chat.autocompact_threshold=0     " disabled
```

The daemon reads the live store at each turn, so a change applies to the next
turn without a restart. It was a session knob until 2026-09-10; it is a tuning
constant with one right answer per install, not a per-session decision.

## RPC surface

- `session.compact` `{session_id}` — sets state to `compacting`; replies
  `{session_id, state, compaction_requested: true}`. Fails with an
  invalid-state error unless the session is `active`.
- `config.set` `{chat: {autocompact_threshold: 0.8}}` — the ordinary config
  write door; `config.unset` returns it to the shipped default.
- Lua: `cru.context.compact(session_id)` wraps the same request as
  `session.compact`, with the same non-effect.
- Recovery: `session.end`, or `session.resume_from_storage`
  `{session_id, kiln}` — revives the session to `active` from any state.

## See also

- [[Help/Concepts/Session Replay]] — session lifecycle features that do work
- [[Meta/Product]] — feature status tracking (see **Auto-Compaction**)
