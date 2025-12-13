---
title: Agent Cards
description: How to define and use agent cards in Crucible
tags:
  - help
  - extending
  - agents
---

# Agent Cards

Agent cards define AI agent personalities and capabilities within your kiln.

## Overview

An agent card is a markdown file in the `Agents/` directory that describes:
- Agent name and role
- System prompt/personality
- Available tools
- Configuration overrides

## File Format

```markdown
---
title: Researcher
description: Research assistant for finding and synthesizing information
type: agent
tools:
  - search
  - read_note
  - semantic_search
model: llama3.2
---

# Researcher

You are a research assistant specializing in finding and synthesizing information...

## Capabilities

- Search notes by keyword
- Semantic similarity search
- Summarize findings
```

## Directory Structure

```
your-kiln/
├── Agents/
│   ├── Researcher.md
│   ├── Coder.md
│   └── Reviewer.md
```

## Using Agents

```bash
# Chat with a specific agent
cru chat --agent Researcher "Find notes about Rust"

# List available agents
cru agents list
```

## See Also

- [[Help/Config/agents]] - Agent configuration
- [[Help/CLI/chat]] - Chat command reference
- [[Agents/Researcher]] - Example agent
