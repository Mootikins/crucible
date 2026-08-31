--!strict
--- Health check example for {{name}} plugin
--- Demonstrates using the cru.health API

local M = {}

function M.check()
    cru.health.start("{{name}}")

    -- Test the MEMBER, never `cru` itself. `cru` is always there, and
    -- `if cru and cru.log` narrows it to nil down the else branch, so the
    -- `cru.health.error` below became a "value could be nil" type error in
    -- the scaffold Crucible hands every new plugin author.
    if cru.log then
        cru.health.ok("cru.log available")
    else
        cru.health.error("cru.log not available")
    end

    if cru.kiln then
        cru.health.ok("cru.kiln available")
    else
        cru.health.warn("cru.kiln not available (optional)")
    end
end

return {
    check = M.check,
}
