--- Health check example for {{name}} plugin
--- Demonstrates using the cru.health API

local M = {}

function M.check()
    cru.health.start("{{name}}")

    if cru and cru.log then
        cru.health.ok("cru.log available")
    else
        cru.health.error("cru.log not available")
    end

    if cru and cru.kiln then
        cru.health.ok("cru.kiln available")
    else
        cru.health.warn("cru.kiln not available (optional)")
    end
end

return {
    check = M.check,
}
