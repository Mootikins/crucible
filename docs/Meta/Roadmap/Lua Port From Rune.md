---
tags: [roadmap, lua, migration]
---

# Lua Port from Rune

Features removed during Rune removal that should be ported to Lua.

## Struct Plugins (just.rn)

**User Story**: As a user, I want to run Just recipes from my kiln via MCP tools.

- `StructPluginHandle` provided `just_*` tools for executing justfile recipes
- File watching for justfile changes to auto-refresh available recipes
- Shell policy enforcement (whitelist/blacklist commands)
- Recipe enrichment via event handlers

**Port approach**: Create a Lua plugin that shells out to `just --list --unsorted` and parses recipes.

## MCP Gateway Manager

**User Story**: As a user, I want to connect upstream MCP servers and expose their tools through Crucible.

- `McpGatewayManager` managed connections to external MCP servers
- Tool prefixing (e.g., `gh_*` for GitHub MCP tools)
- Tool allow/block lists per upstream
- Auto-reconnect on connection loss

**Port approach**: This is protocol-level, not language-specific. Move to crucible-tools as pure Rust.

## Event Pipeline (Rune Plugins)

**User Story**: As a user, I want to filter/transform tool output through custom plugins.

- `EventPipeline` processed `ToolResultEvent` through Rune plugins
- Test output filtering (cargo test, pytest, jest summaries)
- `PluginLoader` discovered plugins from `runes/plugins/` directories

**Port approach**: Lua handlers via `LuaScriptHandlerRegistry` already support this pattern.

## Built-in Handlers

**User Story**: As a user, I want automatic test output summarization and recipe enrichment.

- `builtin:test_filter` - Extracts test summaries from verbose output
- `builtin:recipe_enrichment` - Adds metadata to discovered recipes

**Port approach**: Register as Lua handlers or move to Rust handlers in crucible-core.

## Programmatic Tool Calling

**User Story**: As an agent, I want to call tools programmatically with caching.

- `DashMap`-based tool registry for fast lookup
- Cached tool schemas
- Dynamic tool refresh

**Port approach**: `LuaToolRegistry` already provides this. Ensure parity.

## Priority

1. **High**: MCP Gateway (enables external tool aggregation)
2. **Medium**: Just integration (common workflow)
3. **Low**: Event pipeline (Lua handlers exist)
4. **Low**: Built-in handlers (can be Rust or Lua)
