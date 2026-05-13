---
title: Delegation Patterns
description: Common subagent orchestration recipes — broadcast, router, supervisor — written in plain Lua against cru.sessions primitives
status: implemented
tags:
  - scripting
  - lua
  - agents
  - subagents
  - delegation
---

# Delegation Patterns

Crucible doesn't ship hardcoded "team" types. Three common delegation
patterns — broadcast, router, supervisor — are short Lua recipes against
the existing [[Help/Core/Sessions|cru.sessions]] primitives. This page
is a reference, not a library: copy a recipe, edit it for your case.

The primitives you'll use:

- `cru.sessions.create({ type = "chat", kilns = {...} })` — spawn a
  fresh session (optionally with kilns attached for knowledge access).
- `cru.sessions.configure_agent(id, { agent_name = "..." })` — pick
  which agent profile drives this session.
- `cru.sessions.send_and_collect(id, prompt, { timeout = N })` —
  blocking. Returns an iterator that yields structured response parts:
  `{ type = "text"|"tool_call"|"tool_result"|"thinking", ... }`.
- `cru.sessions.send_message(id, content)` — async dispatch; returns a
  request id. Use this when you want the agent processing in the
  background and don't need to await output inline.
- `cru.sessions.collect_subagents(job_ids, timeout)` — await N
  background subagent jobs (spawned via the daemon's subagent
  infrastructure, distinct from the sessions created above).
- `cru.sessions.fork(id, opts?)` — clone a session's history into a
  new session, e.g. for A/B exploration.
- `cru.sessions.end_session(id)` — clean up.

See [[Help/Core/Sessions]] for full signatures and the
[[Help/Plugins/Lua-Runtime-API|Lua Runtime API]] reference for return
shapes.

## Helper: collect the text from a streamed response

`send_and_collect` returns an iterator over structured parts. Most
recipes want the prose text concatenated:

```lua
local function collect_text(stream)
    local parts = {}
    if not stream then return "" end
    while true do
        local part = stream()
        if not part then break end
        if part.type == "text" then
            parts[#parts + 1] = part.content
        end
    end
    return table.concat(parts, "")
end
```

The `kiln-expert` plugin uses this exact pattern — see
`runtime/plugins/kiln-expert/init.lua` for a working reference.

## Router — classify input, dispatch to one specialist

Use when: input type determines which agent should handle it. Cheap,
single-shot, no coordination overhead.

```lua
local function classify(msg)
    -- Decider can be anything: pattern match, regex, config-driven,
    -- or a small LLM call. This one is pure Lua.
    if msg:match("^!search ") then return "researcher"
    elseif msg:match("```") then return "code-reviewer"
    else return "default"
    end
end

local function route(prompt)
    local agent_name = classify(prompt)
    local s = cru.sessions.create({ type = "chat" })
    cru.sessions.configure_agent(s.id, { agent_name = agent_name })

    local stream = cru.sessions.send_and_collect(s.id, prompt,
        { timeout = 60 })
    local reply = collect_text(stream)
    cru.sessions.end_session(s.id)
    return reply, agent_name
end
```

## Supervisor — iterative decision loop

Use when: a "manager" agent decides which specialist runs next based
on accumulated context. The supervisor is just another session; you
ask it what to do, run that, and feed the output back.

```lua
local function supervise(task, agent_pool, max_steps)
    local sup = cru.sessions.create({ type = "chat" })
    cru.sessions.configure_agent(sup.id, { agent_name = "supervisor" })

    local history = {}
    for step = 1, (max_steps or 10) do
        -- Ask the supervisor what to do next. Encode history however
        -- you like — JSON, bulleted markdown, etc.
        local plan_prompt = string.format(
            'Task: %s\nHistory so far: %s\nAvailable: %s\n' ..
            'Reply with JSON: { "agent": "name", "prompt": "..." } ' ..
            'or { "done": true }.',
            task, cru.json.encode(history),
            table.concat(agent_pool, ", "))
        local plan = collect_text(
            cru.sessions.send_and_collect(sup.id, plan_prompt, { timeout = 60 }))

        local ok, decision = pcall(cru.json.decode, plan)
        if not ok or not decision then break end
        if decision.done then break end

        -- Run the chosen specialist on the chosen sub-prompt.
        local worker = cru.sessions.create({ type = "chat" })
        cru.sessions.configure_agent(worker.id,
            { agent_name = decision.agent })
        local output = collect_text(
            cru.sessions.send_and_collect(worker.id, decision.prompt,
                { timeout = 120 }))
        cru.sessions.end_session(worker.id)

        history[#history + 1] = { agent = decision.agent, output = output }
    end

    cru.sessions.end_session(sup.id)
    return history
end
```

The decider can be anything that returns `{ agent, prompt }` or
`{ done = true }`. Above it's an LLM-judged JSON plan; for a
hard-coded sequence (e.g. researcher → writer → fact-checker), drop
the LLM call and `return history[step]` from a static table.

## Broadcast — fan out to N agents

Use when: multiple agents should weigh in on the same input.

Note: `cru.sessions.send_and_collect` is blocking, so a loop over N
sessions runs sequentially. That's the right answer when each
sub-call is cheap or you don't mind serialised latency:

```lua
local function broadcast_sequential(agents, prompt)
    local results = {}
    for i, agent_name in ipairs(agents) do
        local s = cru.sessions.create({ type = "chat" })
        cru.sessions.configure_agent(s.id, { agent_name = agent_name })
        results[i] = {
            agent = agent_name,
            output = collect_text(
                cru.sessions.send_and_collect(s.id, prompt, { timeout = 60 })),
        }
        cru.sessions.end_session(s.id)
    end
    return results
end
```

For true parallelism, use the subagent-spawning path instead. Tools
that delegate work via `BackgroundJobManager` produce job ids; collect
them all at once:

```lua
-- Inside a tool handler with access to the subagent factory:
local job_ids = {}
for _, agent in ipairs({ "researcher", "skeptic", "writer" }) do
    -- spawn_subagent returns a job id; see the delegate_session tool
    -- and your plugin's subagent factory for the exact call shape.
    job_ids[#job_ids + 1] = spawn_subagent(agent, prompt)
end
local results = cru.sessions.collect_subagents(job_ids, 60)
-- results is { { id, status, output | error, exit_code }, ... }
```

`collect_subagents` waits on background jobs from the daemon's
subagent infrastructure (distinct from sessions created by
`cru.sessions.create`). See [[Help/Concepts/Delegation]] for how to
spawn those jobs and the `delegate_session` tool for the host-side
contract.

## Why no built-in delegation types?

Three reasons:

- **Patterns vary.** Your supervisor might use a Lua decider, an LLM,
  a regex, or a config-driven DAG. A hardcoded `Supervisor` struct
  picks one and shuts out the others.
- **Primitives compose.** `create + configure_agent + send_and_collect
  + end_session` lets you express the three patterns above plus
  pipelines, retries, fan-out-fan-in, chained delegation, and
  workflows the authors didn't think of — anything you can put into
  Lua control flow.
- **Surface area honesty.** Crucible's [Code Principles](../../AGENTS.md)
  call out "no type without a use site." Hardcoded delegation types
  shipped without consumers; recipes ship as docs and stay current
  because users see and edit them.

If a recipe pattern recurs across many of your plugins, fold it into a
helper module — `require("delegation_helpers").router(prompt, classify)`
— and ship that as a plugin. The primitives are the right contract;
opinionated helpers are user-land.
