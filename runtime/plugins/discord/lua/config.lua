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

--- The declared deployment shape: "personal" or "server".
---
--- Reply-chain continuity, self-approval and the `read` tier are each safe in a
--- personal bot and wrong in a shared server. Before this key the only thing
--- separating the two was whether `allowed_guilds` happened to be empty, so the
--- safe configuration was four independent keys an operator had to get mutually
--- coherent. Declaring the shape lets the plugin check them instead.
---
--- Defaults to "personal", the smaller blast radius. An unknown value raises:
--- guessing which one a typo meant is how a server ends up with a personal
--- bot's defaults.
local MODES = { personal = true, server = true }

function M.mode()
    local mode = M.get("mode", "personal")
    if not MODES[mode] then
        error("Discord: unknown mode '" .. tostring(mode) .. "'; expected \"personal\" or \"server\"")
    end
    return mode
end

--- Get gateway intents bitmask
--- Default: GUILDS(0) | GUILD_MESSAGES(9) | DIRECT_MESSAGES(12) | MESSAGE_CONTENT(15)
---
--- Written as shifts, not as a literal. This shipped as 37889 -- bit 10
--- (GUILD_MESSAGE_REACTIONS) where bit 9 was meant -- so the gateway delivered
--- no guild message at all and the bot worked only in DMs, silently, from the
--- commit that shipped the plugin. A wrong bit costs nothing at connect time:
--- Discord accepts the IDENTIFY and simply never sends those events.
local DEFAULT_INTENTS = 1        -- GUILDS          (1 << 0)
    + 512                        -- GUILD_MESSAGES  (1 << 9)
    + 4096                       -- DIRECT_MESSAGES (1 << 12)
    + 32768                      -- MESSAGE_CONTENT (1 << 15)

function M.get_intents()
    return M.get("intents", DEFAULT_INTENTS)
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
