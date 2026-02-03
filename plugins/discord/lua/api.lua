--- Discord REST API wrapper
--- Uses cru.http for all HTTP requests to Discord API v10

local config = require("config")

local M = {}

-- Rate limiter: 5 requests per second (Discord global rate limit)
local limiter = cru.ratelimit.new({ capacity = 5, interval = 1.0 })

-- ---------------------------------------------------------------------------
-- Internal helpers
-- ---------------------------------------------------------------------------

local function api_request(method, path, body)
    limiter:acquire()

    local url = config.api_base() .. path
    local opts = { headers = config.auth_headers() }
    if body then
        opts.body = cru.json.encode(body)
    end

    local resp = cru.retry(function()
        local r
        if method == "GET" then
            r = cru.http.get(url, opts)
        elseif method == "POST" then
            r = cru.http.post(url, opts)
        elseif method == "PUT" then
            r = cru.http.put(url, opts)
        elseif method == "PATCH" then
            r = cru.http.patch(url, opts)
        end

        if r.status == 429 or (r.status >= 500 and r.status < 600) then
            local after = nil
            if r.status == 429 then
                local ok, data = pcall(cru.json.decode, r.body or "")
                if ok and data.retry_after then after = data.retry_after end
            end
            error({ retryable = true, after = after, status = r.status })
        end

        return r
    end, { max_retries = 3, base_delay = 1.0, max_delay = 30.0 })

    if not resp.ok then
        return nil, string.format("Discord API error %d: %s", resp.status, resp.body or "unknown")
    end
    if resp.body and #resp.body > 0 then
        return cru.json.decode(resp.body), nil
    end
    return {}, nil
end

-- ---------------------------------------------------------------------------
-- Channel Messages
-- ---------------------------------------------------------------------------

function M.send_message(channel_id, content, opts)
    opts = opts or {}
    local payload = { content = content }
    if opts.embeds then payload.embeds = opts.embeds end
    if opts.reply_to then payload.message_reference = { message_id = opts.reply_to } end
    return api_request("POST", "/channels/" .. channel_id .. "/messages", payload)
end

function M.get_messages(channel_id, limit, before)
    limit = limit or 50
    local path = "/channels/" .. channel_id .. "/messages?limit=" .. tostring(limit)
    if before then path = path .. "&before=" .. before end
    return api_request("GET", path)
end

-- ---------------------------------------------------------------------------
-- Typing & DM helpers
-- ---------------------------------------------------------------------------

function M.trigger_typing(channel_id)
    return api_request("POST", "/channels/" .. channel_id .. "/typing")
end

function M.create_dm_channel(user_id)
    return api_request("POST", "/users/@me/channels", { recipient_id = user_id })
end

-- ---------------------------------------------------------------------------
-- Channels & Guilds
-- ---------------------------------------------------------------------------

function M.get_channels(guild_id)
    return api_request("GET", "/guilds/" .. guild_id .. "/channels")
end

function M.get_guilds()
    return api_request("GET", "/users/@me/guilds")
end

-- ---------------------------------------------------------------------------
-- Slash Commands (PUT for bulk overwrite per Discord docs)
-- ---------------------------------------------------------------------------

function M.register_global_commands(app_id, commands)
    return api_request("PUT", "/applications/" .. app_id .. "/commands", commands)
end

function M.register_guild_commands(app_id, guild_id, commands)
    return api_request("PUT", "/applications/" .. app_id .. "/guilds/" .. guild_id .. "/commands", commands)
end

-- ---------------------------------------------------------------------------
-- Interactions
-- ---------------------------------------------------------------------------

function M.respond_interaction(interaction_id, interaction_token, response)
    return api_request("POST",
        "/interactions/" .. interaction_id .. "/" .. interaction_token .. "/callback",
        response)
end

function M.edit_interaction_response(app_id, interaction_token, data)
    return api_request("PATCH",
        "/webhooks/" .. app_id .. "/" .. interaction_token .. "/messages/@original",
        data)
end

return M
