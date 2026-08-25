---
title: Vendoring Lua Dependencies
description: How a plugin ships third-party pure-Lua modules — the supported dependency mechanism
status: implemented
tags:
  - plugins
  - lua
  - dependencies
  - reference
aliases:
  - Lua Vendoring
  - Plugin Dependencies
---

# Vendoring Lua Dependencies

Pure-Lua vendoring is the supported dependency mechanism for plugins. To use a
third-party library, copy its `.lua` files into your plugin directory and
`require` them. There is no package manager step and no lockfile: the plugin
directory is the unit of distribution, and it carries everything it needs.

## How to vendor a module

Place the vendored file under a directory named after your own plugin, and
`require` it with that prefix:

```
my-plugin/
├── init.lua
└── my-plugin/
    └── dkjson.lua      -- the vendored library
```

```lua
-- my-plugin/init.lua
local dkjson = require("my-plugin.dkjson")
```

The loader adds `<plugin_dir>/?.lua` and `<plugin_dir>/?/init.lua` to
`package.path` before it runs your `init.lua`, so `require("my-plugin.dkjson")`
resolves to `<plugin_dir>/my-plugin/dkjson.lua`.

## Why the namespace directory

Every plugin shares one `package.path` and one `package.loaded` table. Two
plugins that both vendor `dkjson.lua` at their top level would both say
`require("dkjson")` — and whichever loaded first would answer for both, at
whatever version it happened to ship. The prefix makes the module name unique
to your plugin (`"my-plugin.dkjson"` versus `"other-plugin.dkjson"`), so a
collision is structurally impossible rather than merely unlikely.

## Native modules are unsupported

Native rocks (C libraries built by LuaRocks) cannot load: Crucible's Lua
interpreter is statically vendored into the daemon, so there is no shared
`liblua` for a compiled rock to link against, and the C loaders are removed
from the VM. Vendor a pure-Lua implementation instead — most popular libraries
(JSON, base64, date handling) have one.

## What the runtime already provides

Before vendoring, check what ships in the box: `cru.json` (encode/decode),
`cru.http`, `cru.timer`, `cru.retry`, `cru.emitter`, `cru.check`, and the rest
of the [[Help/Plugins/Lua Runtime API]]. The Lua standard library (`io`, `os`,
`string`, `table`, `math`) is available as itself.
