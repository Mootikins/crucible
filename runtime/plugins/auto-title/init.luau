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

--- The publication channel a session-title provider declares itself on.
--- The daemon reads this key; the plugin's own name is not part of the
--- contract.
local CHANNEL = "session_title"

--- The command the spec declares. A user runs it as `/generate`, or as
--- `/auto-title:generate` when another plugin also declares `generate`.
local COMMAND = "generate"

--- The command the publication names. The full name, so that the daemon
--- finds this command even when another plugin declares a `generate`.
local QUALIFIED = "auto-title:" .. COMMAND

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
local function title_for(session_id: string, user: string, assistant: string?): string
  local answer, err = cru.session.complete(session_id, {
    system = config.prompt or title.SYSTEM_PROMPT,
    prompt = title.exchange(user, assistant, config.clip or title.CLIP),
    timeout = config.timeout,
  })
  if err or not answer then
    error("auto-title: " .. tostring(err or "the model answered with nothing"), 0)
  end
  return title.sanitize(answer)
end

--- The latest user message of a session, and the last assistant reply after it.
---
--- The last reply, not the first: a turn that calls tools writes an assistant
--- row before each call, and the last row is the answer. A message that a
--- plugin injected comes back with the role `system`, so it is not a user turn.
local function last_exchange(session_id: string): (string?, string?)
  local rows, err = cru.session.messages(session_id)
  if not rows then
    error("auto-title: cannot read session " .. session_id .. ": " .. tostring(err), 0)
  end
  local user: string?, assistant: string? = nil, nil
  for _, row in rows do
    if row.role == "user" then
      user, assistant = row.content, nil
    elseif row.role == "assistant" and user then
      assistant = row.content
    end
  end
  return user, assistant
end

--- The command. It has two callers.
---
--- The daemon titles a session after its first turn. It sends the opening
--- exchange as `{ session_id, user, assistant }`, and it persists the title
--- that comes back.
---
--- A user runs `/generate` in a session. The client sends no exchange, so the
--- command titles the session from its latest exchange, and it sets the title
--- itself. The latest exchange, because a user who asks for a new title
--- usually does so when the topic of the session moved.
local function generate(options: { [string]: any }?, ctx: { [string]: any }?): any
  -- Annotate the LOCAL, not the expression. `options or {}` widens to
  -- `T | {}`, and reading a field off that union fails against the empty
  -- half — which is why the whole `args = args or {}` idiom reads as a
  -- defect under a typechecker while being perfectly correct Lua.
  local args: { [string]: any } = options or {}
  if args.user ~= nil then
    local user = args.user
    if type(user) ~= "string" or user == "" then
      error("auto-title: no user message to derive a title from", 0)
    end
    return { title = title_for(args.session_id, user, args.assistant) }
  end

  local context: { [string]: any } = ctx or {}
  local session_id = args.session_id or context.session_id
  if type(session_id) ~= "string" or session_id == "" then
    error("auto-title: no session to title; run /generate in a chat session", 0)
  end
  local user, assistant = last_exchange(session_id)
  if not user or user == "" then
    error("auto-title: session " .. session_id .. " has no user message to title", 0)
  end
  local new_title = title_for(session_id, user, assistant)
  if new_title == "" then
    error("auto-title: the model answered with a blank title", 0)
  end
  local ok, err = cru.session.set_title(session_id, new_title)
  if not ok then
    error("auto-title: cannot set the title: " .. tostring(err), 0)
  end
  return "Session titled: " .. new_title
end

-- Declares the provider, not the title. The plugin publishes the command
-- name, not a value, so the daemon can call back into Lua per session.
--
-- The publication comes from setup(), once. The host calls setup() at
-- activation, under this plugin's source, so the daemon attributes the
-- publication to this plugin. A second setup() call, from the user's
-- init.lua, changes the config and publishes nothing again: the publish
-- binding at that point can name another plugin, and two plugins would then
-- claim `session_title`. The module body registers nothing, so
-- `cru plugin check` reports no top-level effect.
local published = false
local function publish_provider()
  if published then return end
  published = true
  cru.plugin.publish(CHANNEL, { command = QUALIFIED })
end

local plugin = {
  commands = {
    [COMMAND] = {
      desc = "Title this session from its latest exchange",
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
    publish_provider()
  end,
}

return plugin
