--- Discord integration plugin for Crucible
--- Connects to Discord via Gateway WebSocket and REST API.
--- Exposes tools for agents to send/read messages, list channels, and register commands.

local M = {}

local config = require("config")
local api = require("api")
local gateway = require("gateway")

-- ============================================================================
-- Gateway event wiring
-- ============================================================================

gateway.on("MESSAGE_CREATE", function(data)
    if data.author and data.author.bot then return end

    cru.log("info", string.format(
        "Discord message from %s in #%s: %s",
        data.author and data.author.username or "unknown",
        data.channel_id or "?",
        (data.content or ""):sub(1, 80)
    ))
end)

gateway.on("READY", function(data)
    local guild_count = data.guilds and #data.guilds or 0
    cru.log("info", string.format("Discord bot ready: %s (%d guilds)", data.user.username, guild_count))
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
    cru.check.table(commands, "commands")
    if type(commands) == "string" then
        local ok, decoded = pcall(cru.json.decode, commands)
        if not ok then
            return { error = "Failed to parse commands JSON: " .. tostring(decoded) }
        end
        commands = decoded
    end

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
                "Connected (session: %s, seq: %s)",
                info.session_id or "?",
                tostring(info.last_sequence or "?")
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
    version = "0.1.0",
    description = "Discord integration via Gateway WebSocket and REST API",
    capabilities = { "config" },

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
                { name = "commands", type = "table", desc = "Array of command objects" },
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

    setup = function(cfg)
        cru.log("info", "Discord plugin loaded")
    end,
}
