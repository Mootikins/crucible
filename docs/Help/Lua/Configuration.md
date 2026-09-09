---
description: Documentation note for Configuration.
title: Lua Configuration
tags:
  - lua
  - config
  - reference
---

# Lua Configuration

Crucible loads Lua configuration from `~/.config/crucible/init.lua` at startup. This file can configure the TUI, define keybindings, and customize behavior. It runs on the daemon VM, which registers neither `cru.defaults` nor `cru.modes` — those belong to the defaults file, described below.

## Quick Start

Create `~/.config/crucible/init.lua`:

```lua
-- Configure plugins
require("reflection").setup({
  enabled = true,
})

-- Colours
cru.colorscheme.setup({ colors = { primary = "term4" } })

-- Statusline: a row below the input
local sl = cru.statusline
sl.setup({
  prompt = {
    sl.input,
    { sl.mode, " ", sl.model{ max = 25 },
      sl.align,
      sl.any(sl.notification, sl.context) },
  },
})
```

## Config Locations

`~/.config/crucible/init.lua` is the one file you write. It runs once, at
daemon boot, on the daemon VM.

It runs twice, on two VMs, and that is the whole model. At boot it runs on
the daemon VM, where it configures plugins, the statusline and `cru.config`.
On each new session it runs again on that session's VM — **after** the shipped
defaults file (`runtime/defaults/init.lua`, compiled into the binary), which
supplies the default system prompt, the three modes, the precognition
formatter and the plan-mode permission hook.

A VM is its list of sources, executed in order. Nothing merges and nothing
re-applies: a later file wins by ordinary assignment, and `= nil` removes.

| Written in `init.lua` | Effect |
|---|---|
| `cru.defaults.x = …` | every new session starts with `x` |
| `cru.modes.<name> = {…}` | a new mode, in the TUI cycle and the web picker |
| `cru.config.set{…}` | app config |

A workspace runs no Lua. The daemon executes only trees it names in advance,
because naming a tree for the loader is what write-protects it — see
`crucible-daemon/src/execution_roots.rs`. A cloned repository therefore cannot
change a session's model, prompt, modes or hooks.

## The Boot Order

The daemon evaluates your `init.lua` exactly once, at boot, **before** it loads plugins — the Neovim model. Your file authors the config (`cru.config.set`, `runtimepath` included), and plugin *activation* runs afterwards against the final result.

- **Any line may set any config key.** The daemon reads the store when the evaluation finishes, so the last write wins.
- **`cru.config.set` writes one leaf per value.** The nested table is authoring sugar: `cru.config.set({ chat = { model = "x" } })` records the single key `chat.model`, so every other `chat` key stands untouched. An array and a scalar are single values, an empty table sets nothing, and a dotted key names the same path — `{ ["chat.model"] = "x" }` and `{ chat = { model = "x" } }` write one leaf. Neovim core makes the same choice: options are flat, and a deep merge is a library call (`vim.tbl_deep_extend`) a plugin makes for itself.
- **A write never removes a key.** `cru.config.set` adds keys and changes them. To drop a stale `llm.providers.old`, call the `config.unset` RPC with that key: it removes the key and everything under it from the layers a `:set key&` may drop, and it edits no file. A provider your own `init.lua` declares goes away when you delete the line.
- **The module search path is live.** A `runtimepath` entry added on line N serves every `require` after line N — and none before it. The lazy.nvim bootstrap has the same rule: prepend, then require. A failed `require` is never cached, so a retry after the addition succeeds.
- **`require` is a module load, not membership.** It cannot enable, disable, or activate a plugin. A `require` of a disabled plugin still loads its module, but activation registers none of its hooks or exports.
- **Daemon-state APIs raise during evaluation.** `cru.kiln.*`, `cru.session.*`, and storage-backed calls answer "daemon state is not ready during init.lua evaluation; use a hook" — the kiln registry is built *from* your file's output, so it cannot exist during it. Move such reads into a hook.
- **No hot reload.** Runtime `config.set` and `plugin.reload` do not re-run the bootstrap; a `runtimepath` change needs `cru daemon restart`.
- **`require("my.mod")`** resolves from `~/.config/crucible/lua/` everywhere — during boot, in hooks, and in plugins. A module there shadows a same-named plugin module.
- **`config.toml` is not read.** It was the seed under `init.lua` until v0.30.0. The reader is gone, so its values no longer apply. Run `cru config migrate` to move them into Lua.

## Configuring Plugins

Plugins are configured via `require("name").setup({...})` — the same pattern as Neovim plugins.

```lua
-- Configure a bundled plugin with custom settings
require("reflection").setup({
  enabled = true,
  model = "llama3.2",
  timeout = 60,
})
```

The `reflection` block takes these keys. `model` is the only one with no
default; without it the plugin skips every session.

| Key | Default | What it sets |
|-----|---------|--------------|
| `model` | none | The auxiliary model the reviewer runs on |
| `provider` | none | A provider override for the auxiliary model |
| `enabled` | `true` | The master switch |
| `min_turns` | `3` | The fewest user turns a session needs before it is reviewed |
| `max_proposals` | `5` | The most proposals one session may stage |
| `timeout` | `120` | Seconds to wait for the reviewer |
| `rejection_memory` | `20` | How many recent rejected titles the reviewer is told about |
| `tool_result_chars` | `2000` | Characters kept from each tool result in the transcript |
| `transcript_chars` | `60000` | Characters kept from the whole transcript, cut from the front |

The `consolidation` block configures the periodic pass that proposes pattern
notes from several sessions at once. It is **off by default**, because a pass
spends model calls with no user present. `kiln` and `model` have no default.

```lua
require("consolidation").setup({
  enabled = true,
  kiln = "notes",
  model = "llama3.2",
})
```

| Key | Default | What it sets |
|-----|---------|--------------|
| `enabled` | `false` | The master switch, read at each tick |
| `kiln` | none | The kiln the pass reads and stages proposals in |
| `model` | none | The auxiliary model the reviewer runs on |
| `provider` | none | A provider override for the auxiliary model |
| `interval` | `21600` | Seconds between passes; read once, at load |
| `max_problem` | `5` | The most sessions with a tool error or a rejected edit in one pass |
| `max_clean` | `3` | The most clean sessions in one pass; `0` reviews problem sessions only |
| `min_turns` | `2` | The fewest user turns a session needs before the pass reads it |
| `session_chars` | `15000` | Characters kept from each session's transcript |
| `timeout` | `240` | Seconds to wait for the reviewer |
| `rejection_memory` | `20` | How many recent rejected titles the reviewer is told about |

These two tables are the one place the keys are documented. Each plugin's
`cru.plugin.options{}` call declares the same keys and defaults; [[Help/Concepts/Reflection Pass]] says what each pass does with
them.

Plugin configuration has two working forms, and each plugin uses **one**:

- **The direct form** — `require("reflection").setup({...})` at the top of `init.lua`. The call you write *owns* that plugin's setup: activation reuses the same module instance (the file is never evaluated twice) and skips its default `setup(cfg)` call.
- **The store form** — `cru.config.set({ plugins = { reflection = {...} } })`. This feeds the default `setup(cfg)` the activation phase calls for every plugin you did not set up directly.

A plugin configured both ways takes the direct call; pick one form per plugin. Bundled plugins (in `runtime/plugins/`) load with their defaults when you configure nothing. To disable one entirely, set `plugins = { <name> = { enabled = false } }`.

Declaring a git-hosted plugin is a third, separate act: `plugins.declare.<name>` in the same table names a repository the daemon clones and loads at boot. See the plugins section of [[Configuration]] — declarations are not configuration, and the `declare` key is reserved for them.


See [[Help/Extending/Creating Plugins]] for writing your own plugins.

## Session Defaults and Modes

`cru.defaults` sets the value every new session starts with — the Neovim
`vim.o` tier. `session.x` inside a handler changes one session, the `vim.bo`
tier.

```lua
cru.defaults.system_prompt = "Answer in British English."
```

Modes are declared, not built in. `cru.modes.<name>` takes a tool set and a
permission stance; the three shipped modes are declared this same way in the
shipped defaults file, so yours are not second-class. `cru.modes.auto = nil`
removes one, because your file runs after it.

```lua
cru.modes.review = {
  tools = { "read_*", "grep", "glob", "bash" },
  permissions = {
    default = "deny",
    allow = { "bash:rg *", "bash:git log *" },
  },
}
```

Declared modes appear in the TUI's `Shift+Tab` cycle and the web mode picker,
and each gets its own slash command (`/review`). Use a declaration for a static
rule and a hook for one that depends on the arguments — see
[[Help/TUI/Modes]] and [[Help/Concepts/Permission Precedence]].

## Built-in Modules

Every API lives under the one global, `cru`. One pair is easy to mix up:

| Call | Kind | When |
|---|---|---|
| `cru.statusline.setup{}` | config | once, defining the bars |
| `cru.statusline.set(session, key, value)` | runtime | any time, supplying a value |

```lua
-- Runtime namespace
cru.log(level, msg)  -- Logging (debug, info, warn, error)
cru.json.encode(tbl) -- Convert table to JSON string
cru.json.decode(str) -- Parse JSON string to table
cru.include(path)    -- Load another config file

-- Also available via cru.*
cru.http             -- HTTP requests (GET, POST, PUT, etc.)
cru.fs               -- Filesystem operations
cru.shell            -- Shell command execution
cru.oq               -- Data query/transform (parse, json, etc.)
cru.paths            -- Path utilities
cru.ws               -- WebSocket client
cru.kiln             -- Kiln access

-- Utility modules
cru.timer            -- sleep(secs), timeout(secs, fn), clock()
cru.ratelimit        -- Rate limiter: new({capacity, interval})
cru.retry(fn, opts)  -- Exponential backoff retry
cru.emitter.new()    -- Event emitter (:on, :once, :off, :emit)
cru.check            -- Argument validation (.string, .number, .boolean, .table, .func, .one_of)
cru.timer.spawn(fn)  -- Spawn async task (daemon context only, requires send feature)

-- Daemon-side modules (available when running as a plugin in the daemon)
cru.session         -- Session management: create, get, list, send_message, subscribe, etc.

-- UI configuration
cru.colorscheme      -- colour palette
cru.hl               -- highlight groups (set, link)
cru.geometry         -- surface geometry, prompt glyphs, layout
cru.statusline       -- statusline bars and item vocabulary
cru.syntax           -- code highlighting

-- Runtime statusline values (cru only — the counterpart to the config above)
cru.statusline.set   -- push a value for `sl.expr("key")`
cru.statusline.clear -- drop one

-- Legacy aliases (still work)
cru.log         -- same as cru.log
cru.json.encode -- same as cru.json.encode
cru.json.decode -- same as cru.json.decode
-- Standalone globals: http, fs, shell, oq, paths, graph (backwards-compat)
```

> [!warning] `cru.config` and `cru.plugin.config` are not the same function
> `cru.config.get(key)` reads one **top-level** value of the merged app config
> (defaults seeded, `cru.config.set{}` overlaid) and takes no dotted
> paths. On the daemon's plugin VM, `cru.plugin.config.get("plugin.key")` walks
> dotted keys into `plugins.*` config. This is a known trap — check which
> one you mean before reaching for either.

### The keys `cru.config.get` will not return

Config keys whose value names a **filesystem location** are withheld from the
store `cru.config.get` reads, and `cru.config.set{}` will not put them back:

`kilns` · `kiln_path` · `session_kiln` · `projects` · `data_home` ·
`agent_directories` · `runtimepath`

`cru.config.get` returns `nil` for each. The store is the plugin-visible view
of the user's config, and a plugin is told which kilns a session reaches by
*name* — through `session.kilns` and through the `kiln` field on a
`precognition_select` / `precognition_format` result. Publishing the
directories here would be a side door around that. Your own
`plugins.<name>` keys are untouched; the rule is about top-level keys only,
so `cru.plugin.config.get("myplugin.kilns")` still works exactly as before.

The `config.set` RPC refuses the same seven, and reports them in a `rejected`
list. Changing where kilns live is a config-file edit (`cru kiln register`),
not a socket call — the RPC socket has no authentication, and these keys are
the config's answer to *where the daemon acts*.

## Statusline Configuration

The screen is three ordered lists — regions `top`, `prompt`, and `bottom` — and
a region entry is either a **row** (a table of items) or, in `prompt`, the
`sl.input` marker for the editor itself. Position in the list is the
arrangement; there is no anchor and no ordering field:

```lua
local sl = cru.statusline

sl.setup({
  prompt = {
    sl.input,
    { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
      sl.align,
      sl.any(sl.notification, sl.context) },
  },
})
```

Move `sl.input` below a row and that row renders above the editor. A region you
do not mention keeps the built-in default; a key that is not `top`, `prompt`,
or `bottom` (the old `main = {...}` spelling, say) places nothing and logs a
warning.

### Items

| Item | Renders |
|---|---|
| `sl.mode` | the chat mode badge — whatever mode the session is in |
| `sl.model{ max = 25, fallback = "…" }` | the active model, truncated |
| `sl.context` | context-window usage |
| `sl.cache` | prompt-cache hit rate, once one is known |
| `sl.status` | the daemon's status text |
| `sl.notification` | the active toast, or pending counts |
| `sl.align` | an alignment split |
| `sl.expr("key")` | a value pushed from a handler |
| `"any string"` | literal text |

Built-in items are evaluated by the TUI on every frame and cost no RPC.

`:hl("GroupName")` styles any item with a highlight group.

### Conditionals

Lua's `or` does not work here — item objects are truthy, so `a or b` always
takes the first, and the branch has to survive being sent to the client:

```lua
sl.any(sl.notification, sl.context)   -- first one that renders something
sl.when("streaming", sl.cache)        -- only while a turn is streaming
```

Conditions are facts only the TUI knows: `"streaming"`, `"has_notification"`,
and `"mode:<name>"` for any declared mode.

### Values the daemon computes

```lua
sl.setup({ prompt = { sl.input, { sl.mode, sl.align, sl.expr("git") } } })

cru.on("FileChanged", function(ctx)
  local out = cru.shell.exec("git status -b --porcelain")
  cru.statusline.set(ctx.session_id, "git", parse_branch(out))
end)
```

An unset expression renders nothing, so the bar does not jump when the first
value arrives. Re-setting an unchanged value costs no repaint.

See [[Extending/Scripted UI]] for colours, surfaces and borders.

## Example: Full Configuration

```lua
-- ~/.config/crucible/init.lua

-- Colours. `term4` is the terminal's slot 4 — whatever the user put there —
-- rather than a claim that it looks blue.
cru.colorscheme.setup({
  name   = "mine",
  colors = { primary = "term4", success = "term2", text_dim = "bright_black" },
})

cru.hl.set("StatusMode", { fg = "black", bg = "mode_normal", bold = true })

-- Surfaces
cru.geometry.setup({
  popup  = { border = "rounded", padding = 1, max_visible = 10 },
  prompt = { normal = { glyph = "❯ " } },
  layout = { status_bar = "bottom", message_spacing = 1 },
})

-- Code blocks follow the colours above
cru.syntax.setup({ theme = "derived" })

-- Statusline: input first, one row below it
local sl = cru.statusline
sl.setup({
  prompt = {
    sl.input,
    { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
      sl.align,
      sl.any(sl.notification, sl.context) },
  },
})

cru.log("info", "Config loaded!")
```

## Troubleshooting

**Config not loading?**
- Check file exists: `ls ~/.config/crucible/init.lua`
- Check for syntax errors: run `lua ~/.config/crucible/init.lua`
- Check logs: `cru chat` with `RUST_LOG=crucible_lua=debug`

**Statusline or colours not changing?**
- Your `init.lua` is evaluated by the **daemon**, not by `cru`. A stale daemon
  serves stale config — `plugin.reload` re-evaluates it and pushes the result to
  every attached client, so a restart is not required
- Check for syntax errors first; a config that fails to load leaves the built-in
  default in place
- Crucible ships a built-in default that runs before your `init.lua`, so you only
  need to configure what you want to change
- If the daemon is unreachable entirely, the TUI renders from a complete
  compiled-in theme rather than failing
- Check logs with `RUST_LOG=crucible_lua=debug` to verify the config was loaded

## See Also

- [[Scripting Languages]] - Overview of Lua in Crucible
- [[Creating Plugins]] - Writing Lua plugins
