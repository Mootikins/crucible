---
title: Script Agent Queries
description: Status of the planned ask.agent() API for querying an LLM from a script
tags:
  - help
  - extending
  - scripting
  - llm
status: planned
aliases:
  - ask_agent
  - Script LLM Queries
---

# Script Agent Queries

> **Not implemented.** There is no way for a Lua or Fennel script to ask an LLM a question
> today. `ask.agent()` does not exist, and neither does any other `cru.*` binding that
> sends a prompt to a provider. Earlier revisions of this page documented such an API in
> detail; none of it was real.

## What the idea is

A script builds a multiple-choice question batch and hands it to an *agent* rather than a
human. The agent answers, the script branches on the selection. That would let a plugin
delegate a judgement call ("is this note stale?", "which of these three fixes fits?")
without hard-coding heuristics.

## What actually exists

- Delegating a whole *task* to an external agent does work, but from the agent side rather
  than the script side — see [[Help/Concepts/Delegation]] and the `delegate_session` tool.

If you need an LLM decision inside a plugin today, the workable route is to expose the
plugin's work as a tool and let the session's agent make the call, rather than having the
script call out.

## See Also

- [[Help/Extending/Creating Plugins]] — plugin development
- [[Help/Extending/Custom Handlers]] — intercepting and transforming tool calls
