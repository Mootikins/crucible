---
description: Lua scripting reference for Crucible
status: stub
tags:
  - lua
  - luau
  - fennel
  - scripting
  - reference
---

# Lua Language Basics

Crucible uses Luau (Lua with gradual types) for plugin development, with optional Fennel support.

> **Note**: This page is a stub. Full documentation coming soon.

## Why Lua?

Lua is one of the most widely-used scripting languages, with simple syntax that's easy for both humans and LLMs to write. If you want AI to generate your plugins, Lua is an excellent choice.

## Key Features

- **Simple syntax**: Easy to learn if you know JavaScript or Python
- **Gradual types**: Optional type annotations for documentation
- **Fennel support**: Write in Lisp syntax, compile to Lua
- **LLM-friendly**: Models generate high-quality Lua code

## The `cru` Namespace

All built-in modules are accessible under the `cru` namespace (canonical). The `crucible` namespace is a backwards-compatible alias. Standalone globals like `http`, `fs`, `shell`, `oq`, and `paths` also still work.

```lua
-- Canonical access
cru.http.get(url)
cru.fs.read(path)
cru.shell("git", {"status"})
cru.log("info", "message")
cru.json.encode(tbl)
cru.json.decode(str)

-- Aliases (still work)
crucible.log("info", "message")   -- crucible.* alias
http.get(url)                     -- standalone global
```

### Utility Modules

| Module | Description |
|--------|-------------|
| `cru.timer` | `sleep(secs)`, `timeout(secs, fn)` |
| `cru.ratelimit` | `new({capacity, interval})` returning limiter with `:acquire()`, `:try_acquire()`, `:remaining()` |
| `cru.retry(fn, opts)` | Exponential backoff retry (opts: `max_retries`, `base_delay`, `max_delay`, `jitter`, `retryable`) |
| `cru.emitter.new()` | Event emitter with `:on(event, fn)`, `:once(event, fn)`, `:off(event, id)`, `:emit(event, ...)` |
| `cru.check` | Argument validation: `.string(val, name)`, `.number(val, name, opts)`, `.boolean(val, name)`, `.table(val, name)`, `.one_of(val, options, name)` -- all support `{optional=true}` |

## Fennel

Fennel is a Lisp that compiles to Lua. Use `.fnl` files if you prefer Lisp syntax with Lua's runtime.

## Resources

- [Lua Reference Manual](https://www.lua.org/manual/5.4/)
- [Luau Documentation](https://luau-lang.org/)
- [Fennel Language](https://fennel-lang.org/)
- [[Help/Concepts/Scripting Languages]] — Language comparison
- [[Help/Extending/Creating Plugins]] — Plugin development guide

## See Also

- [[Help/Steel/Language Basics]] — Steel reference
