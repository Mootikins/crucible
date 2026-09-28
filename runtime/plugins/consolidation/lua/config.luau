--!strict
--- consolidation configuration
---
--- `cru.settings.new` gives the order, highest priority first: setup()
--- values, then the `plugins.consolidation` section, then `defaults`, then the
--- caller's fallback. The daemon calls setup() with the section at load. A
--- later `setup{}` call from a user lays over it.

local PLUGIN = "consolidation"

local M = {}

-- The keys and what each one sets are documented once, in
-- docs/Help/Lua/Configuration.md ("Configuring Plugins"). `kiln` and
-- `model` have no default, and `enabled` is off: a pass spends model calls
-- with no user present.
local defaults = {
    enabled = false,
    kiln = nil,
    model = nil,
    provider = nil,
    -- The mode of the pass's own session. `propose` makes each note write
    -- a proposal and leaves the disk as it is.
    mode = "propose",
    interval = 21600,
    max_problem = 5,
    max_clean = 3,
    min_turns = 2,
    session_chars = 15000,
    timeout = 240,
}

local settings = cru.settings.new(PLUGIN, defaults)

M.init = settings.init
M.get = settings.get
M.reset = settings.reset

return M
