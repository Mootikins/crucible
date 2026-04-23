---
title: Example Agent Cards
description: Gallery of agent-card templates — copy to activate
tags: [agents, examples]
---

# Example Agent Cards

The markdown files in this directory are **templates**. Crucible does *not* load agent cards from `docs/Agents/` at runtime — the files here exist as a gallery you can copy, adapt, and drop into one of the real load paths.

To use one of these agents:

1. Copy the file to a load path:
   - `~/.config/crucible/agents/` — global (all kilns see it)
   - `<kiln>/.crucible/agents/` — kiln-scoped, hidden
   - `<kiln>/agents/` — kiln-scoped, visible alongside your notes

2. Verify it's picked up:
   ```bash
   cru agents list
   ```

3. Start a chat with it:
   ```bash
   cru chat --agent Researcher
   ```

The format of the frontmatter, tool permissions, MCP bindings, and model specialty field is documented in [[Help/Extending/Agent Cards]].

## The Gallery

| Template | Focus |
|---|---|
| [[Coder]] | Programming tasks, code review, technical notes |
| [[Researcher]] | Knowledge exploration, synthesis, citation |
| [[Reviewer]] | Quality checks on existing notes |
| [[Kiln Specialist]] | Vault-level navigation and organization |
| [[General Assistant]] | Balanced default for everyday use |

[[Tool Capabilities]] catalogs the tool names each agent can declare in its `tools:` block.
