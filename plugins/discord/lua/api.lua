--- Discord REST API wrapper
--- Uses cru.http for all HTTP requests to Discord API v10

local config = require("config")

local M = {}

-- ---------------------------------------------------------------------------
-- Internal helpers
-- ---------------------------------------------------------------------------

local function api_url(path)
    return config.api_base() .. path
end

local function api_get(path)
    local resp = http.get(api_url(path), { headers = config.auth_headers() })
    if not resp.ok then
        return nil, string.format("Discord API error %d: %s", resp.status, resp.body or "unknown")
    end
    return crucible.json_decode(resp.body), nil
end

local function api_post(path, body)
    local resp = http.post(api_url(path), {
        headers = config.auth_headers(),
        body = crucible.json_encode(body),
    })
    if not resp.ok then
        return nil, string.format("Discord API error %d: %s", resp.status, resp.body or "unknown")
    end
    if resp.body and #resp.body > 0 then
        return crucible.json_decode(resp.body), nil
    end
    return {}, nil
end

-- ---------------------------------------------------------------------------
-- Channel Messages
-- ---------------------------------------------------------------------------

--- Send a message to a channel
function M.send_message(channel_id, content, opts)
    opts = opts or {}
    local payload = { content = content }

    if opts.embeds then
        payload.embeds = opts.embeds
    end
    if opts.reply_to then
        payload.message_reference = { message_id = opts.reply_to }
    end

    return api_post("/channels/" .. channel_id .. "/messages", payload)
end

--- Read recent messages from a channel
function M.get_messages(channel_id, limit, before)
    limit = limit or 50
    local path = "/channels/" .. channel_id .. "/messages?limit=" .. tostring(limit)
    if before then
        path = path .. "&before=" .. before
    end
    return api_get(path)
end

-- ---------------------------------------------------------------------------
-- Channels
-- ---------------------------------------------------------------------------

--- List channels in a guild
function M.get_channels(guild_id)
    return api_get("/guilds/" .. guild_id .. "/channels")
end

--- Get a single channel
function M.get_channel(channel_id)
    return api_get("/channels/" .. channel_id)
end

-- ---------------------------------------------------------------------------
-- Guilds
-- ---------------------------------------------------------------------------

--- List guilds the bot is in
function M.get_guilds()
    return api_get("/users/@me/guilds")
end

-- ---------------------------------------------------------------------------
-- Slash Commands
-- ---------------------------------------------------------------------------

--- Register global application commands
function M.register_global_commands(app_id, commands)
    return api_post("/applications/" .. app_id .. "/commands", commands)
end

--- Register guild-specific commands (instant, good for testing)
function M.register_guild_commands(app_id, guild_id, commands)
    return api_post("/applications/" .. app_id .. "/guilds/" .. guild_id .. "/commands", commands)
end

-- ---------------------------------------------------------------------------
-- Interactions
-- ---------------------------------------------------------------------------

--- Respond to an interaction
function M.respond_interaction(interaction_id, interaction_token, response)
    return api_post(
        "/interactions/" .. interaction_id .. "/" .. interaction_token .. "/callback",
        response
    )
end

return M
