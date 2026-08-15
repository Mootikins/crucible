---
title: Lua Eval Command
description: CLI reference for evaluating Lua code in the daemon's plugin runtime.
tags: [help, cli, lua]
---

# cru lua

Evaluate Lua code in the daemon's plugin runtime — the same long-lived VM that plugins
and your `init.lua` run in, so `cru` APIs are available and globals persist between
invocations. The code is sent to the running daemon over the `lua.eval` RPC and
evaluated there, not in the CLI process.

## Synopsis

```
cru lua <code>
cru lua '=<expr>'
cru lua --file <path>
echo 'print(42)' | cru lua -
```

| Argument / Option | Description |
|-------------------|-------------|
| `<code>` | Lua code to evaluate, or `-` to read from stdin |
| `--file <path>` | Read Lua code from a file instead; conflicts with `<code>` |

A leading `=` turns the input into an expression (the daemon prepends `return `, the
Neovim convention): `cru lua '=1+1'` prints `2`, `cru lua '=cru'` dumps the API
namespace. With neither code nor `--file`, the command errors.

## Output

The daemon returns a string rendering of the result: booleans and numbers as-is,
strings unquoted, tables pretty-printed as JSON, other types as `<typename>`. A `nil`
result prints nothing. Lua errors come back as RPC errors and fail the command, as does
a daemon whose Lua runtime is not initialized (plugins disabled).

## Relationship to the TUI `:lua` command

Inside `cru chat`, `:lua <expr>` and its shorthand `:= <expr>` go through the same
`lua.eval` RPC against the same VM — `cru lua` is that command available from a shell.
State is shared: a global you set from the TUI is visible to `cru lua` and vice versa,
because there is one plugin runtime per daemon.

Note that `lua.eval` executes arbitrary code inside the daemon. The daemon socket is
per-user with `0700` permissions, so this is same-user access only.

## See Also

- [[Help/CLI/Index]] — full CLI command reference
