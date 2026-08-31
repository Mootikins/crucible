--!strict
--- auto-title — names a session after what it is about.
---
--- The daemon still decides WHEN a session needs a title (first completed turn,
--- or an explicit `session.generate_title`), still holds the in-flight guard,
--- and still falls back to truncating the first user message when nothing
--- answers. What it no longer holds is any opinion about how a title is
--- ASKED FOR: the prompt, the 1500-character clip and the sanitizer that
--- strips quotes and `Title:` scaffolding all live here, in Lua, editable
--- without a rebuild.
---
--- What this replaces: `crucible-daemon/src/provider/title.rs`.
---
--- The daemon finds this plugin through the `session_title` publication, not
--- by name, so a user plugin publishing the same key replaces the behaviour
--- outright — and a plugin named `auto-title` earlier on the runtimepath
--- shadows this file entirely.
---
--- Configure in `~/.config/crucible/init.lua`:
---
---   require("auto-title").setup({
---     prompt  = "Name this conversation in three words.",
---     clip    = 800,
---     timeout = 20,
---   })
---
--- Or via TOML:
---
---   [plugins.auto-title]
---   clip = 800

local title = require("auto_title")

--- The plugin's name, and the `package.loaded` key the user requires it by.
local NAME = "auto-title"

--- The publication channel a session-title provider declares itself on.
--- The daemon reads this key; the plugin's own name is not part of the
--- contract.
local CHANNEL = "session_title"

--- The command the publication names, and the one the spec declares.
local COMMAND = "auto-title.generate"

--- What `[plugins.auto-title]` may set. Declared, rather than inferred from
--- an empty table: under `--!strict` an empty initializer types `config` as a
--- table with no keys, and every read of one is an error.
export type Config = {
  prompt: string?,
  clip: number?,
  timeout: number?,
}

-- Populated by setup() from `[plugins.auto-title]`, then merged again by any
-- later `setup{}` call from the user's init.lua — Lua beats TOML, and merging
-- rather than replacing means overriding one key keeps the others.
local config: Config = {}

--- Ask the session's own model for a title for one exchange.
---
--- Raises rather than returning a fallback: the daemon owns the fallback (it
--- truncates the first user message), and a plugin inventing a second one
--- would make which of the two you got depend on where the failure happened.
local function generate(args: { [string]: any }?)
  -- Annotate the LOCAL, not the expression. `args or {}` widens to
  -- `T | {}`, and reading a field off that union fails against the empty
  -- half — which is why the whole `args = args or {}` idiom reads as a
  -- defect under a typechecker while being perfectly correct Lua.
  local args: { [string]: any } = args or {}
  local user = args.user
  if type(user) ~= "string" or user == "" then
    error("auto-title: no user message to derive a title from")
  end

  local answer, err = cru.session.complete(args.session_id, {
    system = config.prompt or title.SYSTEM_PROMPT,
    prompt = title.exchange(user, args.assistant, config.clip or title.CLIP),
    timeout = config.timeout,
  })
  if err or not answer then
    error("auto-title: " .. tostring(err or "the model answered with nothing"))
  end

  return { title = title.sanitize(answer) }
end

-- Declares the provider, not the title. Publishing the command name rather
-- than a value is what lets the daemon call back into Lua per session.
--
-- Here in the body rather than in setup(): setup() is the user's entry point
-- and runs again from their init.lua, where `cru.plugin.publish` is bound to
-- whichever plugin the loader executed last. A second publication under
-- someone else's name would make two plugins claim `session_title` — the
-- daemon then warns and picks by name, so configuring the prompt could change
-- who generates the title. The body runs once per load, with the binding the
-- loader set for this plugin.
cru.plugin.publish(CHANNEL, { command = COMMAND })

local plugin = {
  name = NAME,
  version = "0.1.0",
  description = "Name a session after what it is about",

  commands = {
    [COMMAND] = {
      desc = "Generate a topic title for a session's opening exchange",
      fn = generate,
    },
  },

  -- Exposed for the plugin's own tests and for manual invocation.
  sanitize = title.sanitize,
  exchange = title.exchange,
  clip = title.clip,

  setup = function(cfg)
    for k, v in pairs(cfg or {}) do
      config[k] = v
    end
  end,
}

-- The daemon executes this file by path, not through `require`, so nothing
-- would otherwise fill `package.loaded`. The documented
-- `require("auto-title").setup{...}` from the user's init.lua would then load
-- a SECOND copy of this file with its own `config` upvalue, merge into that,
-- and leave the copy the daemon actually calls untouched — a config surface
-- that silently ignores you. Registering the spec here makes that `require`
-- answer with this table.
package.loaded[NAME] = plugin

return plugin
