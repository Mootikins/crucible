--- Discord agent response collection and delivery
--- Routes messages to Crucible sessions and sends responses back to Discord.

local api = require("api")

local M = {}

local MAX_MESSAGE_LEN = 2000
local RESPONSE_TIMEOUT = 120  -- seconds
local TYPING_INTERVAL = 8     -- re-trigger typing every 8s

--- Find the last double-newline (paragraph break) before `pos`.
local function find_paragraph_break(text, pos)
    local best = nil
    local i = 1
    while true do
        local found = text:find("\n\n", i, true)
        if not found or found > pos then break end
        best = found
        i = found + 1
    end
    return best
end

--- Split text into balanced chunks that fit within Discord's message limit.
--- Prefers paragraph boundaries; avoids tiny orphan messages.
local function chunk_text(text, max_len)
    max_len = max_len or MAX_MESSAGE_LEN
    if #text <= max_len then
        return { text }
    end

    local total = #text
    local num_chunks = math.ceil(total / max_len)
    local target_size = math.ceil(total / num_chunks)

    local chunks = {}
    local pos = 1

    while pos <= total do
        local remaining = total - pos + 1
        if remaining <= max_len then
            table.insert(chunks, text:sub(pos))
            break
        end

        local ideal = pos + target_size - 1
        if ideal > pos + max_len - 1 then ideal = pos + max_len - 1 end

        local window = text:sub(pos, ideal)
        local break_at = #window

        local para = find_paragraph_break(window, #window)
        if para and para > #window * 0.4 then
            break_at = para + 1
        else
            local nl = window:find("\n[^\n]*$")
            if nl and nl > #window * 0.3 then
                break_at = nl
            else
                local sp = window:find(" [^ ]*$")
                if sp and sp > #window * 0.3 then
                    break_at = sp
                end
            end
        end

        table.insert(chunks, text:sub(pos, pos + break_at - 1))
        pos = pos + break_at
    end

    return chunks
end

--- Send a user message to a Crucible session and deliver the response to Discord.
---@param session_id string Crucible session ID
---@param channel_id string Discord channel ID
---@param user_message string The user's message content
---@param reply_to_msg_id string|nil Discord message ID to reply to
function M.respond(session_id, channel_id, user_message, reply_to_msg_id)
    cru.log("info", "Responder: starting for session " .. session_id)
    pcall(api.trigger_typing, channel_id)

    local response, err = cru.sessions.send_and_collect(session_id, user_message, {
        timeout = RESPONSE_TIMEOUT,
    })

    cru.log("info", "Responder: collected response (" .. #(response or "") .. " chars)")

    if err then
        cru.log("warn", "Responder: send_and_collect failed: " .. tostring(err))
        api.send_message(channel_id, "Sorry, I couldn't process that: " .. tostring(err), {
            reply_to = reply_to_msg_id,
        })
        return
    end

    if not response or response == "" then
        response = "I didn't have a response for that."
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
