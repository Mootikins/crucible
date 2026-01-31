--- Hermit awareness module
--- Scans kilns, builds profiles, caches results with TTL refresh

local config = require("config")

local M = {}

local cache = {
    profile = nil,
    refreshed_at = 0,
}

local function build_tag_frequency(notes)
    local freq = {}
    for _, note in ipairs(notes) do
        if note.tags then
            for _, tag in ipairs(note.tags) do
                freq[tag] = (freq[tag] or 0) + 1
            end
        end
    end
    return freq
end

local function sorted_tags(freq, limit)
    local tags = {}
    for tag, count in pairs(freq) do
        table.insert(tags, { tag = tag, count = count })
    end
    table.sort(tags, function(a, b) return a.count > b.count end)
    if limit and #tags > limit then
        local trimmed = {}
        for i = 1, limit do
            trimmed[i] = tags[i]
        end
        return trimmed
    end
    return tags
end

local function find_orphans(notes)
    local orphans = {}
    for _, note in ipairs(notes) do
        local outlinks = cru.vault.outlinks(note.path) or {}
        local backlinks = cru.vault.backlinks(note.path) or {}
        if #outlinks == 0 and #backlinks == 0 then
            table.insert(orphans, note.path)
        end
    end
    return orphans
end

local function recent_notes(notes, limit)
    limit = limit or 20
    local sorted = {}
    for _, note in ipairs(notes) do
        table.insert(sorted, note)
    end
    table.sort(sorted, function(a, b)
        return (a.updated_at or "") > (b.updated_at or "")
    end)
    local result = {}
    for i = 1, math.min(limit, #sorted) do
        result[i] = sorted[i]
    end
    return result
end

function M.refresh(force)
    if not force then
        local ttl = config.get("awareness_cache_ttl", 300)
        local age = os.time() - cache.refreshed_at
        if cache.profile and age < ttl then
            return cache.profile
        end
    end

    local notes = cru.vault.list() or {}
    local tag_freq = build_tag_frequency(notes)
    local orphans = find_orphans(notes)
    local recents = recent_notes(notes, 20)

    cache.profile = {
        note_count = #notes,
        tags = sorted_tags(tag_freq, 20),
        tag_freq = tag_freq,
        orphans = orphans,
        orphan_count = #orphans,
        recent = recents,
        refreshed_at = os.date("%Y-%m-%dT%H:%M:%S"),
    }
    cache.refreshed_at = os.time()

    return cache.profile
end

function M.get()
    if cache.profile then
        local ttl = config.get("awareness_cache_ttl", 300)
        local age = os.time() - cache.refreshed_at
        if age < ttl then
            return cache.profile
        end
    end
    return M.refresh(false)
end

function M.invalidate()
    cache.profile = nil
    cache.refreshed_at = 0
end

return M
