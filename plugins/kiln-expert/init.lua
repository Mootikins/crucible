--- kiln-expert — On-demand search across unmounted kilns
---
--- Configure kilns you want searchable but not permanently mounted:
---
---   [plugins.kiln-expert]
---   kilns = { docs = "~/crucible/docs", research = "~/notes/research" }
---   timeout = 30
---
--- The agent gets a `search_kiln` tool. When called, it spins up a
--- subagent session with the target kiln attached, searches, and
--- returns results. The kiln mounts on demand and stays warm in the
--- daemon for subsequent queries.

local config = require("config")

local M = {}

-- ============================================================================
-- Tools
-- ============================================================================

--- Search an on-demand kiln by label.
--- Creates a subagent session with the kiln attached, sends the query,
--- and collects structured results.
function M.search_kiln(args)
    local label = args.kiln
    local query = args.query
    local limit = args.limit or 10

    if not label then
        return { error = "kiln label is required" }
    end
    if not query then
        return { error = "query is required" }
    end

    local kiln_path = config.resolve(label)
    if not kiln_path then
        local available = table.concat(config.labels(), ", ")
        return {
            error = "unknown kiln: " .. label,
            available = available,
        }
    end

    -- Create a throwaway session with the target kiln attached
    local session, err = cru.sessions.create({
        type = "chat",
        kilns = { kiln_path },
    })
    if err then
        return { error = "failed to create session: " .. err }
    end

    -- Ask the subagent to search and return structured results
    local prompt = string.format(
        'Search the attached kiln for: "%s". Return up to %d results. '
            .. "For each result, include the note title, path, and a 1-2 sentence summary of why it matches. "
            .. "Format as a JSON array: [{title, path, summary}]. No other text.",
        query,
        limit
    )

    local response, send_err = cru.sessions.send_and_collect(
        session.id,
        prompt,
        { timeout = config.get("timeout", 30) }
    )

    if send_err then
        cru.sessions.end_session(session.id)
        return { error = "search failed: " .. send_err }
    end

    -- Collect text from response parts
    local text_parts = {}
    if response then
        while true do
            local part = response()
            if not part then break end
            if part.type == "text" then
                text_parts[#text_parts + 1] = part.content
            end
        end
    end

    -- Clean up the session
    cru.sessions.end_session(session.id)

    local result_text = table.concat(text_parts, "")

    -- Try to parse as JSON; fall back to raw text
    local ok, parsed = pcall(cru.json.decode, result_text)
    if ok and parsed then
        return { kiln = label, query = query, results = parsed }
    end

    return { kiln = label, query = query, raw = result_text }
end

--- List available on-demand kilns.
function M.list_kilns(_args)
    local kilns = config.kilns()
    local result = {}
    for label, path in pairs(kilns) do
        result[#result + 1] = { label = label, path = path }
    end
    table.sort(result, function(a, b) return a.label < b.label end)
    return { kilns = result, count = #result }
end

-- ============================================================================
-- Plugin Spec
-- ============================================================================

return {
    name = "kiln-expert",
    version = "0.1.0",
    description = "On-demand search across unmounted kilns via subagent delegation",
    capabilities = { "kiln", "agent", "config" },

    tools = {
        search_kiln = {
            desc = "Search an on-demand kiln by label. Spins up a subagent with the kiln attached.",
            params = {
                { name = "kiln", type = "string", desc = "Kiln label (from config)" },
                { name = "query", type = "string", desc = "Search query" },
                { name = "limit", type = "number", desc = "Max results (default: 10)", optional = true },
            },
            fn = M.search_kiln,
        },
        list_kilns = {
            desc = "List available on-demand kilns",
            fn = M.list_kilns,
        },
    },

    setup = function(cfg)
        if cfg then
            config.init = config.init or function() end
        end
    end,
}
