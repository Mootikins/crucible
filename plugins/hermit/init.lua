--- Hermit — Knowledge Daemon Plugin
--- Watches kilns, connects notes, surfaces forgotten knowledge.

local M = {}

-- Load submodules
local config = require("config")
local awareness = require("awareness")
local background = require("background")
local digest = require("digest")
local reactions = require("reactions")
local ui = require("ui")

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

function M.hermit_digest(args)
    return background.generate_daily_digest(args.days or 1)
end

function M.hermit_links(args)
    if not args.path then
        return { error = "path is required" }
    end
    return background.suggest_links(args.path, args.depth)
end

function M.hermit_orphans(args)
    return background.find_orphans()
end

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

function M.digest_command(args, ctx)
    local profile = awareness.get()
    local result = digest.format(profile, 1)
    ctx:open_view("hermit-digest", { digest = result })
end

function M.soul_command(args, ctx)
    ctx.display_info(load_soul())
end

-- ============================================================================
-- Plugin Spec
-- ============================================================================

return {
    name = "hermit",
    version = "0.1.0",
    description = "Watches your kilns, connects notes, and surfaces forgotten knowledge",
    capabilities = { "vault", "ui", "config", "filesystem" },

    tools = {
        hermit_digest = {
            desc = "Generate an activity summary of the kiln",
            params = {
                { name = "days", type = "number", desc = "Number of days to cover (default: 1)", optional = true },
            },
            fn = M.hermit_digest,
        },
        hermit_links = {
            desc = "Suggest wikilinks for a note",
            params = {
                { name = "path", type = "string", desc = "Path to the note" },
                { name = "depth", type = "number", desc = "Graph traversal depth (default: 2)", optional = true },
            },
            fn = M.hermit_links,
        },
        hermit_orphans = {
            desc = "List notes with no inbound or outbound links",
            fn = M.hermit_orphans,
        },
        hermit_profile = {
            desc = "Show cached kiln profile (note count, tags, orphans, recent)",
            fn = M.hermit_profile,
        },
    },

    commands = {
        hermit = {
            desc = "Hermit knowledge assistant",
            hint = "[status|digest|orphans|soul]",
            fn = M.hermit_command,
        },
        digest = {
            desc = "Show kiln digest",
            hint = "",
            fn = M.digest_command,
        },
        soul = {
            desc = "View Hermit's soul definition",
            hint = "",
            fn = M.soul_command,
        },
    },

    handlers = {
        { name = "on_note_created", event = "note:created", priority = 150, fn = reactions.on_note_created },
        { name = "on_note_modified", event = "note:modified", priority = 150, fn = reactions.on_note_modified },
        { name = "on_session_started", event = "session:started", priority = 50, fn = reactions.on_session_started },
    },

    views = {
        ["hermit-splash"] = { desc = "Hermit kiln overview", fn = ui.splash_view, handler = ui.splash_handler },
        ["hermit-digest"] = { desc = "Hermit daily digest", fn = ui.digest_view, handler = ui.digest_handler },
    },

    setup = function(cfg)
        if cfg then
            config.init(cfg)
        end
    end,
}
