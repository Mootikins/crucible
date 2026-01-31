--- Hermit background operations
--- Link suggestion, orphan detection, digest generation

local awareness = require("awareness")

local M = {}

function M.suggest_links(path, depth)
    depth = depth or 2
    if not path then
        return { error = "Path is required" }
    end

    local note = cru.vault.get(path)
    if not note then
        return { error = "Note not found: " .. path }
    end

    local outlinks = cru.vault.outlinks(path) or {}
    local backlinks = cru.vault.backlinks(path) or {}
    local neighbors = cru.vault.neighbors(path, depth) or {}

    -- Build set of already-linked notes
    local linked = {}
    linked[path] = true
    for _, link in ipairs(outlinks) do
        linked[link] = true
    end
    for _, link in ipairs(backlinks) do
        linked[link] = true
    end

    -- Find unlinked neighbors
    local suggestions = {}
    for _, neighbor in ipairs(neighbors) do
        if not linked[neighbor] then
            table.insert(suggestions, {
                path = neighbor,
                reason = "graph neighbor (depth " .. depth .. ")",
            })
        end
    end

    return {
        note = path,
        existing_outlinks = #outlinks,
        existing_backlinks = #backlinks,
        suggestions = suggestions,
        suggestion_count = #suggestions,
    }
end

function M.find_orphans()
    local profile = awareness.get()
    return {
        orphans = profile.orphans,
        count = profile.orphan_count,
        total_notes = profile.note_count,
    }
end

function M.generate_daily_digest(days)
    days = days or 1
    local profile = awareness.get()
    local digest = require("digest")
    return digest.format(profile, days)
end

return M
