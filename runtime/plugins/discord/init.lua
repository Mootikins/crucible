--- Discord integration plugin for Crucible
--- Connects to Discord via Gateway WebSocket and REST API.
--- Routes @mentions and DMs to Crucible agent sessions for chatbot responses.

local M = {}

local config = require("config")
local gateway = require("gateway")
local sessions = require("sessions")
local responder = require("responder")
local routing = require("routing")
local quota = require("quota")
local api = require("api")

-- Bot identity (captured from READY event)
local bot_user_id = nil

-- ============================================================================
-- Chatbot routing helpers
-- ============================================================================

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

    gateway.update_presence("online", { name = "ready", type = 3 })
end)

gateway.on("MESSAGE_CREATE", function(data)
    local channel_id = data.channel_id

    -- Discord echoes the bot's own messages back over the gateway, and a reply
    -- carries the id of the message it answered. That pair is what indexes a
    -- bot message against the session that produced it — exactly, rather than
    -- guessing from whichever turn was in flight, which two people talking in
    -- one channel would get wrong. Nothing else here concerns our own
    -- messages: `routing.should_respond` drops them anyway.
    if bot_user_id and data.author and data.author.id == bot_user_id then
        sessions.note_bot_message(data.id,
            data.message_reference and data.message_reference.message_id)
        return
    end

    -- A reply to an outstanding permission prompt, before any routing — it is
    -- an answer, not a new turn, and must not cost a quota charge. Anything
    -- that is not an answer falls through and is treated as an ordinary
    -- message. The authorization lives in `try_resolve_permission`: only the
    -- account the prompt named resolves it, wherever it was shown.
    if responder.try_resolve_permission(channel_id,
        data.author and data.author.id, data.content) then
        return
    end

    if not routing.should_respond(data, bot_user_id) then return end

    local content = clean_content(data.content)
    if content == "" then return end

    local guild_id = data.guild_id
    local msg_id = data.id
    local author_id = data.author and data.author.id

    -- Above `get_or_create` so a throttled user never causes a session to
    -- exist, and inline so the check and the increment are one synchronous
    -- critical section — `cru.spawn` is a real `tokio::spawn`, so a flood run
    -- through it could charge the same turn twice. Only the refusal reply is
    -- spawned, and only for the message that crosses the cap.
    local within_quota, refusal = quota.charge(author_id)
    if not within_quota then
        if refusal then
            cru.spawn(function()
                pcall(api.send_message, channel_id, refusal,
                    { reply_to = guild_id and msg_id or nil })
            end)
        end
        return
    end

    -- `referenced_message` is the resolved reply target and arrives on the
    -- event itself; `message_reference` is the id alone, and is what survives
    -- when the target could not be resolved. Either identifies the session a
    -- reply should continue.
    local reply_to = (data.referenced_message and data.referenced_message.id)
        or (data.message_reference and data.message_reference.message_id)

    -- The sender's role ids in this guild, which `access_tier` reads `role:`
    -- grants from. Present on every guild MESSAGE_CREATE; a DM has no `member`
    -- at all, and a role id would mean nothing outside its guild anyway.
    local roles = data.member and data.member.roles

    local session_id, err = sessions.get_or_create(channel_id, guild_id, author_id, {
        message_id = msg_id,
        reply_to = reply_to,
        roles = roles,
    })
    if not session_id then
        cru.log("warn", "Failed to get session for channel " .. channel_id .. ": " .. tostring(err))
        return
    end

    local interactive = sessions.tier_is_interactive(sessions.access_tier(guild_id, author_id, roles))

    cru.spawn(function()
        local reply_to = guild_id and msg_id or nil
        local ok, resp_err = pcall(
            responder.respond, session_id, channel_id, content, reply_to, author_id, interactive
        )
        if not ok then
            cru.log("warn", "Responder error: " .. tostring(resp_err))
        end
    end)
end)

-- Runs on every receive-loop iteration, so it stays inline: `cru.spawn` is a
-- real `tokio::spawn`, and one task per iteration would pile up faster than
-- they finish.
gateway.set_periodic_hook(function()
    sessions.cleanup_stale()
end)

-- ============================================================================
-- Commands (user-facing)
-- ============================================================================

--- Plugin commands are invoked as `fn(args)` with a single table and their
--- return value is rendered by the caller; there is no display context, and
--- nothing in the tree produces a positional argument list.
function M.discord_command(args)
    local sub = type(args) == "table" and type(args.input) == "string"
        and args.input:match("^%s*(%S*)")
        or nil
    if not sub or sub == "" then sub = "status" end

    if sub == "connect" then
        if gateway.is_connected() then
            return { status = "Already connected to Discord gateway." }
        end
        local ok, err = pcall(gateway.connect)
        if not ok then
            return { error = "Discord gateway error: " .. tostring(err) }
        end
        -- `gateway.connect` blocks until a clean disconnect or an exhausted
        -- retry budget, so reaching here means the connection is over.
        return { status = "Discord gateway connection ended." }

    elseif sub == "disconnect" then
        gateway.disconnect()
        return { status = "Disconnected from Discord gateway." }

    elseif sub == "status" then
        local info = gateway.session_info()
        if not info.connected then
            return { status = "Not connected. Use :discord connect" }
        end
        return {
            status = "Connected",
            session_id = info.session_id,
            last_sequence = info.last_sequence,
            active_sessions = sessions.active_count(),
        }
    end

    return { error = "Unknown subcommand: " .. sub .. ". Try: connect, disconnect, status" }
end

-- ============================================================================
-- Plugin Spec
-- ============================================================================

local plugin = {
    name = "discord",
    version = "0.2.0",
    description = "Discord integration via Gateway WebSocket and REST API",
    capabilities = { "config", "network", "websocket", "agent" },

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
            -- Fail closed, and do it here rather than in the spawner: every
            -- declared service is spawned unconditionally on load, and
            -- `gateway.connect` dials Discord before any token is read. The
            -- resulting token error is a *table*, which the retry predicate
            -- accepts, so an unconfigured daemon spent its whole retry budget
            -- against a third party. A lazily-evaluated closure is required —
            -- spec extraction stubs `cru` with an `__index` that returns a
            -- function, so any `cru.x.y` at file scope raises and lands the
            -- plugin in `PluginState::Error`.
            fn = function()
                if not config.get("auto_connect", false) then
                    cru.log("info", "Discord: auto_connect is false; gateway not started")
                    return
                end
                -- `config.get_token` errors when unset, so this is a pcall and
                -- not a truthiness check.
                if not pcall(config.get_token) then
                    cru.log("info", "Discord: no bot_token configured; gateway not started")
                    return
                end
                -- A personal bot with guilds listed is a contradiction, and the
                -- dangerous resolution is the quiet one: answer the DMs, drop
                -- every guild message, look healthy. Refuse and name the key --
                -- a silently deaf bot is the bug the intents default already
                -- was, and it went unnoticed from the day the plugin shipped.
                local ok_mode, mode = pcall(config.mode)
                if not ok_mode then
                    cru.log("warn", "Discord: " .. tostring(mode) .. "; gateway not started")
                    return
                end
                local guilds = config.get("allowed_guilds", {})
                if mode == "personal" and type(guilds) == "table" and #guilds > 0 then
                    cru.log("warn",
                        "Discord: allowed_guilds is set but mode is 'personal', so no guild "
                        .. "message would ever be answered. Set mode = \"server\" to run in "
                        .. "servers, or clear allowed_guilds. Gateway not started.")
                    return
                end
                gateway.connect()
            end,
        },
    },

    setup = function(cfg)
        cru.log("info", "Discord plugin loaded")
    end,
}

-- The daemon executes this file by path, not through `require`, so nothing
-- would otherwise fill `package.loaded`. `tests/service_test.lua` does
-- `require("discord")`, which without this loads a SECOND copy: the two
-- `gateway.on` calls at body level run again, and `Emitter:on` appends rather
-- than replaces, so every Discord message would be handled twice — two agent
-- turns, two replies, two quota charges. Claiming the entry makes that
-- `require` answer with this table.
package.loaded["discord"] = plugin

return plugin
