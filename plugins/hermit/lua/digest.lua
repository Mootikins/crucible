--- Hermit digest formatting
--- Renders awareness profiles into structured markdown/data

local M = {}

function M.format(profile, days)
    days = days or 1

    local lines = {}
    table.insert(lines, "# Kiln Digest")
    table.insert(lines, "")
    table.insert(lines, string.format("Generated: %s", os.date("%Y-%m-%d %H:%M")))
    table.insert(lines, "")

    -- Overview
    table.insert(lines, "## Overview")
    table.insert(lines, "")
    table.insert(lines, string.format("- **Notes**: %d", profile.note_count))
    table.insert(lines, string.format("- **Orphans**: %d", profile.orphan_count))
    table.insert(lines, string.format("- **Top tags**: %d tracked", #profile.tags))
    table.insert(lines, string.format("- **Last scan**: %s", profile.refreshed_at or "unknown"))
    table.insert(lines, "")

    -- Top tags
    if #profile.tags > 0 then
        table.insert(lines, "## Top Tags")
        table.insert(lines, "")
        local limit = math.min(10, #profile.tags)
        for i = 1, limit do
            local entry = profile.tags[i]
            table.insert(lines, string.format("- `%s` (%d)", entry.tag, entry.count))
        end
        table.insert(lines, "")
    end

    -- Recent notes
    if profile.recent and #profile.recent > 0 then
        table.insert(lines, "## Recent Activity")
        table.insert(lines, "")
        local limit = math.min(10, #profile.recent)
        for i = 1, limit do
            local note = profile.recent[i]
            local title = note.title or note.path
            table.insert(lines, string.format("- [[%s]]", title))
        end
        table.insert(lines, "")
    end

    -- Orphans
    if profile.orphan_count > 0 then
        table.insert(lines, "## Orphaned Notes")
        table.insert(lines, "")
        table.insert(lines, "These notes have no inbound or outbound links:")
        table.insert(lines, "")
        local limit = math.min(15, #profile.orphans)
        for i = 1, limit do
            table.insert(lines, string.format("- [[%s]]", profile.orphans[i]))
        end
        if #profile.orphans > limit then
            table.insert(lines, string.format("- ...and %d more", #profile.orphans - limit))
        end
        table.insert(lines, "")
    end

    return {
        markdown = table.concat(lines, "\n"),
        note_count = profile.note_count,
        orphan_count = profile.orphan_count,
        tag_count = #profile.tags,
        recent_count = profile.recent and #profile.recent or 0,
    }
end

return M
