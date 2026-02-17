---
name: crucible-help
description: Crucible documentation and help context for agents
license: MIT
compatibility: ">=0.1.0"
allowed-tools: search_notes get_note
---

# Crucible

A local-first AI assistant where every conversation becomes a searchable, linkable note you own. No cloud, no lock-in — your chats live as markdown files wired into a knowledge graph you control.

## Key Concepts

**Kilns** — A directory of markdown files that Crucible treats as a connected knowledge base. Any folder with `.md` files becomes a kiln when you run `cru init`. No proprietary formats — files stay portable to any editor. See [[Help/Concepts/Kilns]].

**Wikilinks** — `[[Note Name]]` syntax connects notes into a knowledge graph. Supports aliases (`[[Note|display]]`), heading refs (`[[Note#Section]]`), and block refs (`[[Note#^id]]`). Embeds use `![[Note]]`. See [[Help/Wikilinks]].

**Sessions as Markdown** — Every chat saves to a `.md` file in your kiln. Sessions are searchable, linkable, and version-controlled in git. Resume any session with `cru chat --resume <id>`.

**Precognition** — Auto-RAG that injects relevant kiln context before each agent turn. Always on by default. This is the core differentiator: every conversation is knowledge-graph-aware. Toggle with `:set precognition`.

**ACP (Agent Context Protocol)** — Crucible spawns external agents (Claude, OpenCode, Gemini, Codex) and gives them access to your knowledge graph, semantic search, and tools. Use `cru chat -a claude`. See [[Help/Concepts/Agents & Protocols]].

**MCP (Model Context Protocol)** — Expose your kiln as tools for any MCP-compatible AI. Run `cru mcp` to start the server. Tools include `semantic_search`, `create_note`, `read_note`, `list_notes`, and more.

**Semantic Search** — Block-level vector embeddings enable paragraph-granularity search with reranking. Combine with full-text and property search for comprehensive discovery.

## Quick Reference

| Command | Description |
|---------|-------------|
| `cru chat` | Start interactive AI chat |
| `cru chat -a claude` | Chat via Claude Code agent |
| `cru mcp` | Start MCP server |
| `cru process` | Parse and index kiln notes |
| `cru session list` | List recent sessions |
| `cru stats` | Show kiln statistics |
| `cru init` | Initialize a new kiln |

**In-chat:** `/plan` (read-only), `/act` (write), `:model` (switch model), `:set thinkingbudget=high` (thinking), `BackTab` (cycle modes).

## Tools Available

When chatting, agents access: `semantic_search`, `text_search`, `property_search`, `read_note`, `list_notes`. In act mode: `create_note`, `update_note`, `delete_note`.

## Further Reading

- [[Help/CLI/chat]] — Full chat command reference
- [[Help/CLI/Index]] — All CLI commands
- [[Help/Concepts/Kilns]] — Kiln deep dive
- [[Help/Wikilinks]] — Wikilink syntax reference
- [[Help/Extending/Creating Plugins]] — Lua plugin system