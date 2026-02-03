--- Discord agent response collection and delivery
--- Routes messages to Crucible sessions and sends responses back to Discord.

local api = require("api")

local M = {}

local MAX_MESSAGE_LEN = 2000
local RESPONSE_TIMEOUT = 120  -- seconds
local TYPING_INTERVAL = 8     -- re-trigger typing every 8s

--- Split text into chunks that fit within Discord's message limit.
--- Breaks at newlines or spaces when possible.
local function chunk_text(text, max_len)
    max_len = max_len or MAX_MESSAGE_LEN
    if #text <= max_len then
        return { text }
    end

    local chunks = {}
    local remaining = text

    while #remaining > 0 do
        if #remaining <= max_len then
            table.insert(chunks, remaining)
            break
        end

        -- Find a good break point: prefer newline, then space, then hard cut
        local break_at = max_len
        local nl = remaining:sub(1, max_len):find("\n[^\n]*$")
        if nl and nl > max_len * 0.3 then
            break_at = nl
        else
            local sp = remaining:sub(1, max_len):find(" [^ ]*$")
            if sp and sp > max_len * 0.3 then
                break_at = sp
            end
        end

        table.insert(chunks, remaining:sub(1, break_at))
        remaining = remaining:sub(break_at + 1)
    end

    return chunks
end

--- Collect the full agent response using a pre-created event iterator.
--- Reads streaming events until completion or timeout, then unsubscribes.
---@param next_event function Event iterator from cru.sessions.subscribe
---@param session_id string Crucible session ID (for unsubscribe)
---@param opts table|nil Optional: { timeout = number, on_waiting = function }
---@return string response The collected response text
---@return string|nil error Error message if failed
function M.collect_response_with_iterator(next_event, session_id, opts)
    opts = opts or {}
    local timeout = opts.timeout or RESPONSE_TIMEOUT

    local parts = {}
    local start_time = cru.timer.clock()

    while true do
        if opts.on_waiting then
            opts.on_waiting()
        end

        if cru.timer.clock() - start_time > timeout then
            cru.log("warn", "Response timeout for session " .. session_id)
            break
        end

        -- Call next_event() directly — it's an async function that yields to tokio.
        -- Do NOT wrap in cru.timer.timeout() since async functions can't yield
        -- through a regular Lua function boundary.
        local event, event_err = next_event()

        if event_err then
            cru.log("warn", "Collector: event error: " .. tostring(event_err))
            break
        end

        -- nil means stream ended
        if event == nil then break end

        -- Skip non-table values
        if type(event) ~= "table" then
            goto continue_loop
        end

        local event_type = event.type or event.event

        if event_type == "text_delta" then
            local text = event.data and event.data.text or event.data
            if type(text) == "string" then
                table.insert(parts, text)
            end
        elseif event_type == "message_complete" or event_type == "response_complete" or event_type == "response_done" then
            break
        elseif event_type == "error" then
            local err_msg = event.data and event.data.message or "Unknown error"
            table.insert(parts, "\n[Error: " .. tostring(err_msg) .. "]")
            break
        end

        ::continue_loop::
    end

    pcall(cru.sessions.unsubscribe, session_id)

    local response = table.concat(parts)
    if response == "" then
        response = "I didn't have a response for that."
    end

    return response, nil
end

--- Send a user message to a Crucible session and deliver the response to Discord.
---@param session_id string Crucible session ID
---@param channel_id string Discord channel ID
---@param user_message string The user's message content
---@param reply_to_msg_id string|nil Discord message ID to reply to
function M.respond(session_id, channel_id, user_message, reply_to_msg_id)
    cru.log("info", "Responder: starting for session " .. session_id)
    pcall(api.trigger_typing, channel_id)

    -- Subscribe BEFORE sending the message to avoid missing early events
    cru.log("info", "Responder: subscribing to events")
    local next_event, sub_err = cru.sessions.subscribe(session_id)
    if not next_event then
        cru.log("warn", "Responder: subscribe failed: " .. tostring(sub_err))
        api.send_message(channel_id, "Sorry, I couldn't connect to the session: " .. tostring(sub_err), {
            reply_to = reply_to_msg_id,
        })
        return
    end
    cru.log("info", "Responder: subscribed, sending message")

    local msg_id, err = cru.sessions.send_message(session_id, user_message)
    if not msg_id then
        cru.log("warn", "Responder: send_message failed: " .. tostring(err))
        pcall(cru.sessions.unsubscribe, session_id)
        api.send_message(channel_id, "Sorry, I couldn't process that: " .. tostring(err), {
            reply_to = reply_to_msg_id,
        })
        return
    end
    cru.log("info", "Responder: message sent (id=" .. msg_id .. "), collecting response")

    -- Keep typing indicator alive while collecting the response
    local last_typing = cru.timer.clock()
    local response, collect_err = M.collect_response_with_iterator(next_event, session_id, {
        on_waiting = function()
            if cru.timer.clock() - last_typing > TYPING_INTERVAL then
                pcall(api.trigger_typing, channel_id)
                last_typing = cru.timer.clock()
            end
        end,
    })

    cru.log("info", "Responder: collected response (" .. #(response or "") .. " chars)")

    if collect_err then
        api.send_message(channel_id, "Sorry, I lost the connection to the agent.", {
            reply_to = reply_to_msg_id,
        })
        return
    end

    local chunks = chunk_text(response)

    for i, chunk in ipairs(chunks) do
        local opts = {}
        if i == 1 and reply_to_msg_id then
            opts.reply_to = reply_to_msg_id
        end
        local _, send_err = api.send_message(channel_id, chunk, opts)
        if send_err then
            cru.log("warn", "Failed to send response chunk: " .. tostring(send_err))
            break
        end
    end
end

return M
