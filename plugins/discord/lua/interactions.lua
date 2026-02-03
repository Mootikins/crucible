--- Discord slash command and interaction handler
--- Handles INTERACTION_CREATE events for /ask, /search, /status.

local api = require("api")
local config = require("config")
local sessions = require("sessions")
local responder = require("responder")

local M = {}

-- Discord interaction types
local INTERACTION_TYPE = {
    PING = 1,
    APPLICATION_COMMAND = 2,
    MESSAGE_COMPONENT = 3,
    AUTOCOMPLETE = 4,
    MODAL_SUBMIT = 5,
}

-- Discord interaction callback types
local CALLBACK = {
    PONG = 1,
    CHANNEL_MESSAGE = 4,
    DEFERRED_CHANNEL_MESSAGE = 5,
    DEFERRED_UPDATE = 6,
    UPDATE_MESSAGE = 7,
}

--- Extract an option value from interaction data by name.
local function get_option(data, name)
    local options = data.data and data.data.options
    if not options then return nil end
    for _, opt in ipairs(options) do
        if opt.name == name then
            return opt.value
        end
    end
    return nil
end

--- Send a deferred response (acknowledges within 3s, edit later).
local function defer_response(interaction_id, token, ephemeral)
    local flags = ephemeral and 64 or 0
    return api.respond_interaction(interaction_id, token, {
        type = CALLBACK.DEFERRED_CHANNEL_MESSAGE,
        data = { flags = flags },
    })
end

--- Edit the deferred response with final content.
local function edit_deferred(app_id, token, content, embeds)
    local data = {}
    if content then data.content = content end
    if embeds then data.embeds = embeds end
    return api.edit_interaction_response(app_id, token, data)
end

--- Send an immediate ephemeral response.
local function ephemeral_response(interaction_id, token, content)
    return api.respond_interaction(interaction_id, token, {
        type = CALLBACK.CHANNEL_MESSAGE,
        data = { content = content, flags = 64 },
    })
end

-- ============================================================================
-- Command handlers
-- ============================================================================

--- /ask <question> — route to agent session, return response
local function handle_ask(data)
    local app_id = config.get("app_id", "")
    local question = get_option(data, "question")
    if not question or question == "" then
        return ephemeral_response(data.id, data.token, "Please provide a question.")
    end

    -- Defer so we have 15 min to respond
    defer_response(data.id, data.token, false)

    local session_id, err = sessions.get_or_create(data.channel_id, data.guild_id)
    if not session_id then
        return edit_deferred(app_id, data.token, "Failed to create session: " .. tostring(err))
    end

    local msg_id, send_err = cru.sessions.send_message(session_id, question)
    if not msg_id then
        return edit_deferred(app_id, data.token, "Agent error: " .. tostring(send_err))
    end

    local response, collect_err = responder.collect_response(session_id)
    if collect_err then
        return edit_deferred(app_id, data.token, "Failed to collect response: " .. collect_err)
    end

    -- Discord edit limit is 2000 chars; truncate if needed
    if #response > 2000 then
        response = response:sub(1, 1990) .. "\n[truncated]"
    end

    edit_deferred(app_id, data.token, response)
end

--- /search <query> — search kiln, return results as embed
local function handle_search(data)
    local app_id = config.get("app_id", "")
    local query = get_option(data, "query")
    if not query or query == "" then
        return ephemeral_response(data.id, data.token, "Please provide a search query.")
    end

    defer_response(data.id, data.token, false)

    local results, err = cru.kiln.search(query)
    if not results then
        return edit_deferred(app_id, data.token, "Search failed: " .. tostring(err))
    end

    if #results == 0 then
        return edit_deferred(app_id, data.token, "No results found for: " .. query)
    end

    -- Format results as embed fields
    local fields = {}
    for i = 1, math.min(10, #results) do
        local r = results[i]
        table.insert(fields, {
            name = r.title or r.path or ("Result " .. i),
            value = (r.snippet or r.path or ""):sub(1, 200),
            inline = false,
        })
    end

    edit_deferred(app_id, data.token, nil, {
        {
            title = "Search: " .. query,
            description = string.format("%d result(s) found", #results),
            color = 0x7C3AED,  -- purple
            fields = fields,
        },
    })
end

--- /status — ephemeral message with session info
local function handle_status(data)
    local session_count = sessions.active_count()
    local msg = string.format("Active Discord sessions: %d", session_count)

    -- Try to get kiln stats
    local ok, stats = pcall(cru.kiln.list)
    if ok and stats then
        msg = msg .. string.format("\nNotes in kiln: %d", #stats)
    end

    ephemeral_response(data.id, data.token, msg)
end

-- ============================================================================
-- Public API
-- ============================================================================

--- Route an INTERACTION_CREATE event to the appropriate handler.
function M.handle(data)
    if not data or not data.type then return end

    if data.type == INTERACTION_TYPE.PING then
        return api.respond_interaction(data.id, data.token, { type = CALLBACK.PONG })
    end

    if data.type ~= INTERACTION_TYPE.APPLICATION_COMMAND then return end

    local command_name = data.data and data.data.name
    if not command_name then return end

    if command_name == "ask" then
        handle_ask(data)
    elseif command_name == "search" then
        handle_search(data)
    elseif command_name == "status" then
        handle_status(data)
    else
        cru.log("info", "Unknown slash command: " .. command_name)
    end
end

--- Register slash commands with Discord (called on READY).
function M.register_commands(app_id)
    local commands = {
        {
            name = "ask",
            description = "Ask Crucible a question",
            options = {
                {
                    name = "question",
                    description = "Your question",
                    type = 3,  -- STRING
                    required = true,
                },
            },
        },
        {
            name = "search",
            description = "Search your Crucible kiln",
            options = {
                {
                    name = "query",
                    description = "Search query",
                    type = 3,  -- STRING
                    required = true,
                },
            },
        },
        {
            name = "status",
            description = "Show Crucible bot status",
        },
    }

    local result, err = api.register_global_commands(app_id, commands)
    if err then
        cru.log("warn", "Failed to register commands: " .. tostring(err))
    else
        cru.log("info", "Registered " .. #commands .. " slash command(s)")
    end
    return result, err
end

return M
