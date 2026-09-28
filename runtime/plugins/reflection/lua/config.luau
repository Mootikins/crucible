--!strict
--- reflection configuration
---
--- `cru.settings.new` gives the order, highest priority first: setup()
--- values, then the `plugins.reflection` section, then `defaults`, then the
--- caller's fallback. The daemon calls setup() with the section at load. A
--- later `setup{}` call from a user lays over it.

local PLUGIN = "reflection"

local M = {}

-- The keys and what each one sets are documented once, in
-- docs/Help/Lua/Configuration.md ("Configuring Plugins"). `model` has no
-- default: without one the plugin skips every session.
local defaults = {
    enabled = true,
    model = nil,
    provider = nil,
    -- The mode of the pass's own session. `propose` makes each note write
    -- a proposal and leaves the disk as it is.
    mode = "propose",
    min_turns = 3,
    timeout = 120,
    tool_result_chars = 2000,
    transcript_chars = 60000,
}

local settings = cru.settings.new(PLUGIN, defaults)

M.init = settings.init
M.get = settings.get
M.reset = settings.reset

return M
