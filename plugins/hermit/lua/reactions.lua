--- Hermit event reactions
--- Handlers for note lifecycle and session events

local config = require("config")
local awareness = require("awareness")

local M = {}

--- Suggest links when a new note is created
function M.on_note_created(ctx, event)
    if not config.reaction_enabled("note_created") then
        return event
    end
    if not config.get("auto_link", true) then
        return event
    end

    local path = event.path or (event.note and event.note.path)
    if not path then
        return event
    end

    local outlinks = cru.kiln.outlinks(path) or {}
    local backlinks = cru.kiln.backlinks(path) or {}
    local neighbors = cru.kiln.neighbors(path, 2) or {}

    local suggestions = {}
    local linked = {}
    for _, link in ipairs(outlinks) do
        linked[link] = true
    end
    for _, link in ipairs(backlinks) do
        linked[link] = true
    end

    for _, neighbor in ipairs(neighbors) do
        if not linked[neighbor] and neighbor ~= path then
            table.insert(suggestions, neighbor)
        end
    end

    if #suggestions > 0 and not config.quiet() then
        local msg = string.format(
            "New note has %d neighbor(s) worth linking: %s",
            #suggestions,
            table.concat(suggestions, ", ")
        )
        cru.log("info", msg)
    end

    -- Invalidate cache since the collection changed
    awareness.invalidate()

    return event
end

--- Check for broken links when a note is modified
function M.on_note_modified(ctx, event)
    if not config.reaction_enabled("note_modified") then
        return event
    end

    local path = event.path or (event.note and event.note.path)
    if not path then
        return event
    end

    local outlinks = cru.kiln.outlinks(path) or {}
    local broken = {}

    for _, link in ipairs(outlinks) do
        local target = cru.kiln.get(link)
        if not target then
            table.insert(broken, link)
        end
    end

    if #broken > 0 and not config.quiet() then
        local msg = string.format(
            "Broken link(s) in %s: %s",
            path,
            table.concat(broken, ", ")
        )
        cru.log("warn", msg)
    end

    -- Invalidate cache since content changed
    awareness.invalidate()

    return event
end

--- Bootstrap awareness on session start
function M.on_session_started(ctx, event)
    if not config.reaction_enabled("session_started") then
        return event
    end

    local profile = awareness.refresh(true)

    if not config.quiet() then
        local msg = string.format(
            "%d notes, %d orphans. The collection grows.",
            profile.note_count,
            profile.orphan_count
        )
        cru.log("info", msg)
    end

    return event
end

return M
