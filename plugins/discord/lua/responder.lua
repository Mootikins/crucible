--- Discord agent response collection and delivery
--- Routes messages to Crucible sessions and sends responses back to Discord.

local api = require("api")

local M = {}

local MAX_MESSAGE_LEN = 2000
local RESPONSE_TIMEOUT = 120  -- seconds
local TYPING_INTERVAL = 8     -- re-trigger typing every 8s

--- Find structural break positions in text up to `limit`, scored by priority:
--- 3 = heading (\n#), 2 = paragraph (\n\n), 1 = single newline.
--- Each entry: {pos = byte where next section starts, priority = int}.
local function find_structural_breaks(text, limit)
    local breaks = {}
    local i = 1
    while i <= limit do
        local nl = text:find("\n", i, true)
        if not nl or nl > limit then break end

        local next_char = text:sub(nl + 1, nl + 1)
        if next_char == "\n" then
            -- Consume consecutive blank lines, break pos = start of next content
            local end_blanks = nl + 1
            while text:sub(end_blanks + 1, end_blanks + 1) == "\n" do
                end_blanks = end_blanks + 1
            end
            local has_heading = text:sub(end_blanks + 1, end_blanks + 1) == "#"
            table.insert(breaks, { pos = end_blanks + 1, priority = has_heading and 3 or 2 })
            i = end_blanks + 1
        elseif next_char == "#" then
            table.insert(breaks, { pos = nl + 1, priority = 3 })
            i = nl + 1
        else
            table.insert(breaks, { pos = nl + 1, priority = 1 })
            i = nl + 1
        end
    end
    return breaks
end

--- Split text into balanced chunks that fit within Discord's message limit.
--- Prefers heading and paragraph boundaries; avoids tiny orphan messages.
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
        local all_breaks = find_structural_breaks(window, #window)

        local min_offset = math.floor(#window * 0.3)
        local best_pos = nil
        local best_priority = -1
        local best_dist = math.huge

        for _, b in ipairs(all_breaks) do
            if b.pos >= min_offset then
                local dist = math.abs(b.pos - target_size)
                if b.priority > best_priority
                    or (b.priority == best_priority and dist < best_dist) then
                    best_pos = b.pos
                    best_priority = b.priority
                    best_dist = dist
                end
            end
        end

        local break_at
        if best_pos then
            break_at = best_pos - 1
        else
            local sp = window:find(" [^ ]*$")
            if sp and sp > min_offset then
                break_at = sp
            else
                break_at = #window
            end
        end

        local chunk = text:sub(pos, pos + break_at - 1):gsub("%s+$", "")
        if #chunk > 0 then
            table.insert(chunks, chunk)
        end
        pos = pos + break_at
        -- Skip inter-chunk whitespace, but preserve heading markers
        while pos <= total and (text:sub(pos, pos) == "\n" or text:sub(pos, pos) == " ") do
            if text:sub(pos, pos) == "#" then break end
            pos = pos + 1
        end
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
