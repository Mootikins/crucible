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

Place dependencies in the plugin's private `lua/` directory:

```
my-plugin/
├── init.luau
└── lua/
    └── dkjson.lua      -- the vendored library
```

```lua
-- my-plugin/init.luau
local dkjson = require("dkjson")
```

The host resolves this to `<plugin_dir>/lua/dkjson.lua`. Private modules are
cached by path: two plugins can vendor different versions under the same
module name without sharing an instance. Public plugin modules are cached by
name in `package.loaded`. There is no `package.path` or Lua searcher chain to modify.

## Native modules are unsupported

Native Lua/C rocks cannot load. Vendor a pure-Lua implementation compatible
with Luau instead; a library requiring LuaJIT or another Lua version may need changes.

## What the runtime already provides

Before vendoring, check what ships in the box: `cru.json` (encode/decode),
`cru.http`, `cru.timer`, `cru.retry`, `cru.emitter`, `cru.check`, and the rest
of the [[Help/Plugins/Lua Runtime API]]. See [[Help/Lua/Language Basics]] for
host-provided compatibility APIs and omissions.
