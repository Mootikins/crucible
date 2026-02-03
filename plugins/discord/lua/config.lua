--- Discord plugin configuration helpers

local M = {}

local API_BASE = "https://discord.com/api/v10"
local GATEWAY_URL = "wss://gateway.discord.gg/?v=10&encoding=json"

-- Cached bot token (resolved once per runtime)
local cached_token = nil

--- Get a config value with default fallback
function M.get(key, default)
    local ok, val = pcall(function()
        return crucible.config.get("discord." .. key)
    end)
    if ok and val ~= nil then return val end
    return default
end

--- Get bot token from config (cached after first call)
function M.get_token()
    if cached_token then return cached_token end

    local token = M.get("bot_token", "")
    if token == "" then
        token = os.getenv("DISCORD_BOT_TOKEN") or ""
    end
    if token == "" then
        error("Discord bot token not configured. Set discord.bot_token in config or DISCORD_BOT_TOKEN env var.")
    end

    cached_token = token
    return token
end

--- Get gateway intents bitmask
function M.get_intents()
    return M.get("intents", 33281)
end

--- Authorization headers for REST API
function M.auth_headers()
    return {
        ["Authorization"] = "Bot " .. M.get_token(),
        ["Content-Type"] = "application/json",
        ["User-Agent"] = "DiscordBot (crucible, 0.1.0)",
    }
end

function M.api_base() return API_BASE end
function M.gateway_url() return GATEWAY_URL end

return M
