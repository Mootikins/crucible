---
title: AI Features
description: Map of AI agent capabilities, chat, and protocol integrations
status: implemented
tags:
  - moc
  - agents
  - chat
  - ai
---

# AI Features

Crucible integrates AI throughout the experience. This map connects all AI-powered capabilities.

## Chat Interface

Interactive AI conversations with your knowledge:

- [[Help/CLI/chat]] - Chat command and modes
- [[Help/Concepts/Agents & Protocols]] - Understanding MCP and ACP

## Agent System

Configure AI behavior for specific tasks:

- [[Help/Extending/Agent Cards]] - Define agent personas and tools
- [[Agents/Researcher]] - Example: deep exploration agent
- [[Agents/Coder]] - Example: code-focused agent
- [[Agents/Reviewer]] - Example: quality review agent

## Tool Integration

Give agents access to capabilities:

- [[Help/Extending/Custom Tools]] - Create MCP tools
- [[Help/Extending/MCP Gateway]] - Connect external MCP servers
- [[Help/Extending/Event Hooks]] - React to agent actions

## Provider Configuration

Set up AI backends:

- [[Help/Config/llm]] - LLM provider configuration
- [[Help/Config/embedding]] - Embedding configuration
- [[Help/Config/agents]] - Agent-specific settings

## How It Works

- [[Help/Concepts/Semantic Search]] - How AI finds relevant content
- [[Help/Concepts/Agents & Protocols]] - MCP vs ACP explained

## Invisible Helpers

Small daemon-side features that shape what the model and the UI see:

### Tool output filtering

**Status: removed 2026-08-18.** The daemon carried summarizers for six test
runners (cargo test, pytest, Jest, go test, RSpec, Elixir Mix) that nothing in
the tool dispatch path ever called, so no output was ever filtered. They were
deleted rather than wired up: which lines of a test run matter, and how many of
them, is a product opinion, and it belongs in a plugin a user can edit rather
than in a Rust file a user must send a patch to.

The place to rebuild it is the `tool_result` hook, which fires over the outcome
**as the model will receive it**, accepts a `{ result = ... }` patch, and
filters by tool name. See [[Help/Extending/Event Hooks]].

### Unlinked-mention detection (autolink)

The daemon can scan text for kiln note names that appear as plain prose but
are not already wikilinked, and suggest `[[links]]`. Matching is
case-insensitive and word-boundary aware (underscores don't count as
boundaries), skips names shorter than 3 characters, skips notes the text
already links, ignores matches inside existing `[[...]]` (including
`[[target|alias]]` and `[[target#heading]]` forms), and returns at most one
suggestion per note name. Non-ASCII text or note names whose lowercase form
changes byte length are skipped rather than mislocated.

It runs on demand, not automatically: the daemon RPC `suggest_links` takes a
kiln and a text, matches against every note name in that kiln, and returns
suggestions. The web UI's backlinks panel uses it to show a note's "unlinked
mentions" with self-mentions filtered out. Nothing rewrites a note
automatically: clicking a suggestion in that panel inserts the wikilink into
the open editor buffer, which you still save.

## Related

- [[Extending Crucible]] - All extension points
- [[Configuration]] - Full configuration reference

## See Also

- [[Index]] - Return to main index
- `:h chat` - Chat command help
- `:h agents` - Agent configuration help
