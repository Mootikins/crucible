--- Discord integration plugin for Crucible
--- Connects to Discord via Gateway WebSocket and REST API.
--- Exposes tools for agents to send/read messages, list channels, and register commands.
--- Routes @mentions and DMs to Crucible agent sessions for chatbot responses.

local M = {}

local config = require("config")
local api = require("api")
local gateway = require("gateway")
local sessions = require("sessions")
local responder = require("responder")

-- Bot identity (captured from READY event)
local bot_user_id = nil

-- ============================================================================
-- Chatbot routing helpers
-- ============================================================================

--- Check if a message should trigger a bot response.
local function should_respond(data)
    -- Never respond to bots
    if data.author and data.author.bot then return false end

    local content = data.content or ""
    local respond_to = config.get("respond_to", "mentions")

    -- DMs (guild_id is nil for DMs)
    if not data.guild_id then return true end

    -- Check @mention
    if bot_user_id and respond_to ~= "prefix" then
        if content:find("<@" .. bot_user_id .. ">") or content:find("<@!" .. bot_user_id .. ">") then
            return true
        end
    end

    -- Check prefix
    local prefix = config.get("command_prefix", "")
    if prefix ~= "" and (respond_to == "prefix" or respond_to == "both") then
        if content:sub(1, #prefix) == prefix then
            return true
        end
    end

    -- Respond to all messages in channel
    if respond_to == "all" then return true end

    return false
end

--- Strip bot mention and command prefix from message content.
local function clean_content(content)
    if not content then return "" end

    -- Strip @mention
    if bot_user_id then
        content = content:gsub("<@!?" .. bot_user_id .. ">", "")
    end

    -- Strip command prefix
    local prefix = config.get("command_prefix", "")
    if prefix ~= "" and content:sub(1, #prefix) == prefix then
        content = content:sub(#prefix + 1)
    end

    return content:match("^%s*(.-)%s*$") or ""
end

-- ============================================================================
-- Gateway event wiring
-- ============================================================================

gateway.on("READY", function(data)
    bot_user_id = data.user and data.user.id
    local guild_count = data.guilds and #data.guilds or 0
    cru.log("info", string.format("Discord bot ready: %s (%d guilds)", data.user.username, guild_count))

    -- Auto-register slash commands if app_id is configured
    local app_id = config.get("app_id", "")
    if app_id ~= "" then
        local interactions = require("interactions")
        local ok, err = pcall(interactions.register_commands, app_id)
        if not ok then
            cru.log("warn", "Failed to register slash commands: " .. tostring(err))
        end
    end
end)

gateway.on("MESSAGE_CREATE", function(data)
    if data.author and data.author.bot then return end

    local channel_id = data.channel_id
    local raw = (data.content or ""):match("^%s*(.-)%s*$") or ""
    local lower = raw:lower()

    -- Intercept y/n replies for pending permission prompts
    if responder.pending_replies[channel_id] == "waiting" then
        if lower == "y" or lower == "yes" then
            responder.pending_replies[channel_id] = true
            return
        elseif lower == "n" or lower == "no" then
            responder.pending_replies[channel_id] = false
            return
        end
    end

    if not should_respond(data) then return end

    local content = clean_content(data.content)
    if content == "" then return end

    local guild_id = data.guild_id
    local msg_id = data.id

    local session_id, err = sessions.get_or_create(channel_id, guild_id)
    if not session_id then
        cru.log("warn", "Failed to get session for channel " .. channel_id .. ": " .. tostring(err))
        return
    end

    cru.spawn(function()
        local reply_to = guild_id and msg_id or nil
        local ok, resp_err = pcall(responder.respond, session_id, channel_id, content, reply_to)
        if not ok then
            cru.log("warn", "Responder error: " .. tostring(resp_err))
        end
    end)
end)

-- Wire periodic hooks (digest + session cleanup)
local digest = require("digest")
gateway.set_periodic_hook(function()
    digest.maybe_send()
    sessions.cleanup_stale()
end)

gateway.on("INTERACTION_CREATE", function(data)
    local interactions = require("interactions")
    local ok, err = pcall(interactions.handle, data)
    if not ok then
        cru.log("warn", "Interaction handler error: " .. tostring(err))
    end
end)

-- ============================================================================
-- Tools (exposed to agents)
-- ============================================================================

function M.discord_send(args)
    cru.check.string(args.channel_id, "channel_id")
    cru.check.string(args.content, "content")
    cru.check.string(args.reply_to, "reply_to", { optional = true })

    if #args.content > 2000 then
        return { error = "Message content exceeds 2000 character limit" }
    end

    local result, err = api.send_message(args.channel_id, args.content, {
        reply_to = args.reply_to,
    })
    if err then return { error = err } end

    return {
        id = result.id,
        channel_id = result.channel_id,
        content = result.content,
        timestamp = result.timestamp,
    }
end

function M.discord_read(args)
    cru.check.string(args.channel_id, "channel_id")
    cru.check.number(args.limit, "limit", { optional = true, min = 1, max = 100 })
    cru.check.string(args.before, "before", { optional = true })

    local messages, err = api.get_messages(args.channel_id, args.limit, args.before)
    if err then return { error = err } end

    local result = {}
    for _, msg in ipairs(messages) do
        table.insert(result, {
            id = msg.id,
            author = msg.author and msg.author.username or "unknown",
            author_id = msg.author and msg.author.id,
            content = msg.content,
            timestamp = msg.timestamp,
            is_bot = msg.author and msg.author.bot or false,
            reply_to = msg.referenced_message and msg.referenced_message.id,
        })
    end

    return { messages = result, count = #result }
end

function M.discord_channels(args)
    if not args.guild_id then
        local guilds, err = api.get_guilds()
        if err then return { error = err } end
        return { error = "guild_id is required. Available guilds:", guilds = guilds }
    end

    local channels, err = api.get_channels(args.guild_id)
    if err then return { error = err } end

    local result = {}
    for _, ch in ipairs(channels) do
        local type_name = ({
            [0] = "text", [2] = "voice", [4] = "category",
            [5] = "announcement", [15] = "forum",
        })[ch.type] or "other"

        table.insert(result, {
            id = ch.id,
            name = ch.name,
            type = type_name,
            type_id = ch.type,
            position = ch.position,
            parent_id = ch.parent_id,
            topic = ch.topic,
        })
    end

    table.sort(result, function(a, b) return (a.position or 0) < (b.position or 0) end)
    return { channels = result, count = #result }
end

function M.discord_register_commands(args)
    cru.check.string(args.app_id, "app_id")
    cru.check.string(args.guild_id, "guild_id", { optional = true })

    local commands = args.commands
    if type(commands) == "string" then
        local ok, decoded = pcall(cru.json.decode, commands)
        if not ok then
            return { error = "Failed to parse commands JSON: " .. tostring(decoded) }
        end
        commands = decoded
    end
    cru.check.table(commands, "commands")

    local result, err
    if args.guild_id then
        result, err = api.register_guild_commands(args.app_id, args.guild_id, commands)
    else
        result, err = api.register_global_commands(args.app_id, commands)
    end
    if err then return { error = err } end

    return { registered = true, result = result }
end

-- ============================================================================
-- Commands (user-facing)
-- ============================================================================

function M.discord_command(args, ctx)
    local sub = args._positional and args._positional[1] or "status"

    if sub == "connect" then
        if gateway.is_connected() then
            ctx.display_info("Already connected to Discord gateway.")
            return
        end
        ctx.display_info("Connecting to Discord gateway...")
        local ok, err = pcall(gateway.connect)
        if not ok then
            ctx.display_error("Discord gateway error: " .. tostring(err))
        end

    elseif sub == "disconnect" then
        gateway.disconnect()
        ctx.display_info("Disconnected from Discord gateway.")

    elseif sub == "status" then
        local info = gateway.session_info()
        if info.connected then
            ctx.display_info(string.format(
                "Connected (session: %s, seq: %s, active sessions: %d)",
                info.session_id or "?",
                tostring(info.last_sequence or "?"),
                sessions.active_count()
            ))
        else
            ctx.display_info("Not connected. Use :discord connect")
        end

    else
        ctx.display_error("Unknown subcommand: " .. sub .. ". Try: connect, disconnect, status")
    end
end

-- ============================================================================
-- Plugin Spec
-- ============================================================================

return {
    name = "discord",
    version = "0.2.0",
    description = "Discord integration via Gateway WebSocket and REST API",
    capabilities = { "config", "network", "websocket", "agent" },

    tools = {
        discord_send = {
            desc = "Send a message to a Discord channel",
            params = {
                { name = "channel_id", type = "string", desc = "Discord channel ID" },
                { name = "content", type = "string", desc = "Message content (max 2000 chars)" },
                { name = "reply_to", type = "string", desc = "Message ID to reply to", optional = true },
            },
            fn = M.discord_send,
        },
        discord_read = {
            desc = "Read recent messages from a Discord channel",
            params = {
                { name = "channel_id", type = "string", desc = "Discord channel ID" },
                { name = "limit", type = "number", desc = "Max messages (1-100, default 50)", optional = true },
                { name = "before", type = "string", desc = "Message ID for pagination", optional = true },
            },
            fn = M.discord_read,
        },
        discord_channels = {
            desc = "List channels in a Discord guild (server)",
            params = {
                { name = "guild_id", type = "string", desc = "Discord guild (server) ID" },
            },
            fn = M.discord_channels,
        },
        discord_register_commands = {
            desc = "Register slash commands with Discord",
            params = {
                { name = "app_id", type = "string", desc = "Discord application ID" },
                { name = "guild_id", type = "string", desc = "Guild ID (for testing)", optional = true },
                { name = "commands", type = "string", desc = "JSON array of command objects (or table)" },
            },
            fn = M.discord_register_commands,
        },
    },

    commands = {
        discord = {
            desc = "Discord gateway management",
            hint = "[connect|disconnect|status]",
            fn = M.discord_command,
        },
    },

    services = {
        gateway = {
            desc = "Discord WebSocket gateway connection",
            fn = gateway.connect,
        },
    },

    setup = function(cfg)
        cru.log("info", "Discord plugin loaded")
    end,
}
