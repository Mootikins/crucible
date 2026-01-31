--- Hermit — Knowledge Daemon Plugin
--- Watches kilns, connects notes, surfaces forgotten knowledge.

local M = {}

-- Load submodules
local config = require("config")
local awareness = require("awareness")
local background = require("background")
local digest = require("digest")

-- ============================================================================
-- SOUL loader
-- ============================================================================

local soul_content = nil

local function load_soul()
    if soul_content then
        return soul_content
    end

    local custom = config.get("soul_file", "")
    local path = custom ~= "" and custom or "soul.md"

    local file = io.open(path, "r")
    if file then
        soul_content = file:read("*all")
        file:close()
    else
        soul_content = "You are Hermit, a quiet knowledge curator."
    end

    return soul_content
end

-- ============================================================================
-- Session hook
-- ============================================================================

crucible.on_session_start(function(session)
    load_soul()
    awareness.refresh(false)
end)

-- ============================================================================
-- Tools
-- ============================================================================

--- Generate a kiln activity digest
-- @tool name="hermit_digest" desc="Generate an activity summary of the kiln"
-- @param days number? "Number of days to cover (default: 1)"
function M.hermit_digest(args)
    return background.generate_daily_digest(args.days or 1)
end

--- Suggest wikilinks for a note based on graph neighbors
-- @tool name="hermit_links" desc="Suggest wikilinks for a note"
-- @param path string "Path to the note"
-- @param depth number? "Graph traversal depth (default: 2)"
function M.hermit_links(args)
    if not args.path then
        return { error = "path is required" }
    end
    return background.suggest_links(args.path, args.depth)
end

--- List orphaned notes with no connections
-- @tool name="hermit_orphans" desc="List notes with no inbound or outbound links"
function M.hermit_orphans(args)
    return background.find_orphans()
end

--- Show the cached kiln awareness profile
-- @tool name="hermit_profile" desc="Show cached kiln profile (note count, tags, orphans, recent)"
function M.hermit_profile(args)
    local profile = awareness.get()
    return {
        note_count = profile.note_count,
        orphan_count = profile.orphan_count,
        top_tags = profile.tags,
        recent = profile.recent,
        refreshed_at = profile.refreshed_at,
    }
end

-- ============================================================================
-- Commands
-- ============================================================================

--- /hermit master command
-- @command name="hermit" desc="Hermit knowledge assistant" hint="[status|digest|orphans|soul]"
function M.hermit_command(args, ctx)
    local sub = args._positional and args._positional[1] or "status"

    if sub == "status" then
        ctx:open_view("hermit-splash")
    elseif sub == "digest" then
        local profile = awareness.get()
        local result = digest.format(profile, 1)
        ctx:open_view("hermit-digest", { digest = result })
    elseif sub == "orphans" then
        local result = background.find_orphans()
        if result.count == 0 then
            ctx.display_info("No orphaned notes. Every note has at least one connection.")
        else
            local lines = { string.format("Orphaned notes (%d):", result.count) }
            for _, path in ipairs(result.orphans) do
                table.insert(lines, "  [[" .. path .. "]]")
            end
            ctx.display_info(table.concat(lines, "\n"))
        end
    elseif sub == "soul" then
        ctx.display_info(load_soul())
    else
        ctx.display_error("Unknown subcommand: " .. sub .. ". Try: status, digest, orphans, soul")
    end
end

--- /digest shortcut
-- @command name="digest" desc="Show kiln digest" hint=""
function M.digest_command(args, ctx)
    local profile = awareness.get()
    local result = digest.format(profile, 1)
    ctx:open_view("hermit-digest", { digest = result })
end

--- /soul shortcut
-- @command name="soul" desc="View Hermit's soul definition" hint=""
function M.soul_command(args, ctx)
    ctx.display_info(load_soul())
end

return M
