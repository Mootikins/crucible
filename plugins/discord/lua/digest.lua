--- Proactive kiln digest for Discord
--- Periodically sends an embed with recent kiln activity to configured channels.

local api = require("api")
local config = require("config")

local M = {}

-- Track when last digest was sent
local last_digest_at = 0

--- Check if it's time to send a digest, and send if so.
function M.maybe_send()
    local interval = config.get("digest_interval", 0)
    if interval <= 0 then return end

    local now = os.time()
    if now - last_digest_at < interval then return end

    local channels = config.get("digest_channels", {})
    if type(channels) ~= "table" or #channels == 0 then return end

    local embed = M.build_digest()
    if not embed then return end

    last_digest_at = now

    for _, channel_id in ipairs(channels) do
        local _, err = api.send_message(channel_id, nil, { embeds = { embed } })
        if err then
            cru.log("warn", "Failed to send digest to channel " .. tostring(channel_id) .. ": " .. tostring(err))
        end
    end
end

--- Build a Discord embed with recent kiln activity.
function M.build_digest()
    local ok, notes = pcall(cru.kiln.list)
    if not ok or not notes or #notes == 0 then return nil end

    table.sort(notes, function(a, b)
        return (a.updated_at or "") > (b.updated_at or "")
    end)

    -- Build recent notes list (top 5)
    local lines = {}
    for i = 1, math.min(5, #notes) do
        local title = notes[i].title or notes[i].name or notes[i].path or "Untitled"
        table.insert(lines, string.format("- **%s**", title))
    end

    local description = table.concat(lines, "\n")

    -- Append graph stats if available
    local stat_ok, stats = pcall(cru.graph.stats)
    if stat_ok and stats then
        description = description .. string.format(
            "\n\nNodes: %s | Edges: %s",
            tostring(stats.nodes or stats.node_count or "?"),
            tostring(stats.edges or stats.edge_count or "?")
        )
    end

    return {
        title = "Kiln Digest",
        description = description,
        color = 0x7C3AED,
        footer = {
            text = string.format("%d total notes", #notes),
        },
        timestamp = os.date("!%Y-%m-%dT%H:%M:%SZ"),
    }
end

return M
