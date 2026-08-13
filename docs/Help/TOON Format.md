---
title: TOON Format
description: Token-Oriented Object Notation, the compact encoding Crucible uses for some tool results
tags: [help, format]
---

# TOON Format

**TOON** — Token-Oriented Object Notation — is a compact, human-readable encoding for
structured data destined for an LLM. It carries the same information as JSON in noticeably
fewer tokens by hoisting repeated object keys into a single header row.

JSON:

```json
{
  "users": [
    { "id": 1, "name": "Alice" },
    { "id": 2, "name": "Bob" }
  ]
}
```

The same value in TOON:

```
users[2]{id,name}:
  1,Alice
  2,Bob
```

The array length and the field names are declared once; each row is just the values.

TOON is an external specification, not a Crucible invention. Crucible encodes it through
the [`oq`](https://crates.io/crates/oq) crate, which wraps
[`toon-format`](https://crates.io/crates/toon-format) — the reference Rust implementation of
[TOON v3.0](https://github.com/toon-format/spec/blob/main/SPEC.md).

## Where Crucible uses it

**Lua tool results served over `cru mcp`.** The MCP server (`cru mcp`, and the
daemon-managed server it mirrors) discovers Lua and Fennel tools from the plugin
directories by their spec-table declarations, and encodes their object or array
results as TOON before returning them to the connected client
(`crates/crucible-daemon/src/tools/toon_response.rs`, called from
`tools/extended_mcp_server.rs`). Scalar results (strings, numbers, booleans) pass through
as plain text.

That is the only path that encodes TOON. A tool declared in a plugin's `tools` spec table
and called from a Crucible chat session is dispatched by `PluginToolExecutor`
(`crates/crucible-daemon/src/plugin_tools.rs`), which returns the Lua value converted
straight to JSON. Built-in Rust tools return JSON too.

The formatter is content-aware: long fields such as a file's contents or a command's
stdout are lifted out into readable blocks rather than being packed into a row, and the
strategy is chosen from the tool's name (a name containing `search`/`find`/`grep` is
formatted as search results, and so on). If encoding fails for any reason, the value falls
back to pretty-printed JSON — a tool result is never lost to a formatting error.

## Producing TOON from Lua

The `cru.oq` module exposes the encoder directly, alongside the other formats it handles:

```lua
local rows = {
  { id = 1, name = "Alice" },
  { id = 2, name = "Bob" },
}

print(cru.oq.toon({ users = rows }))       -- TOON string
print(cru.oq.convert(rows, "yaml"))        -- any supported format by name
print(cru.oq.format({ users = rows }))     -- smart TOON, long fields extracted
print(cru.oq.format(result, { tool = "search_notes" }))  -- tool-aware formatting
```

`cru.oq` also parses (`parse`, `parse_as`, `detect`), re-encodes (`json`, `json_pretty`,
`yaml`, `toml`), and runs jq-style queries (`query`). See
[[Help/Plugins/Lua Runtime API]] for the wider `cru.*` surface.

## See Also

- [[Help/Plugins/Lua Runtime API]] — the `cru.oq` module in context
- [[Help/Extending/Custom Tools]] — writing the Lua tools whose results get TOON-encoded
