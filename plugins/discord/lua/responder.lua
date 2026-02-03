--- Discord agent response collection and delivery
--- Routes messages to Crucible sessions and streams response parts back to Discord.

local api = require("api")
local tables = require("tables")

local M = {}

local MAX_MESSAGE_LEN = 2000
local RESPONSE_TIMEOUT = 120  -- seconds
local PERMISSION_TIMEOUT = 60 -- seconds to wait for y/n reply
local TYPING_INTERVAL = 8    -- seconds between typing indicator refreshes

-- Pending permission replies: channel_id -> {state="waiting"|"allowed"|"denied", user_id=string}
-- Set by responder, resolved by init.lua when it intercepts a y/n from the same user.
M.pending_replies = {}

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

--- Format tool call args in a clean, tool-specific way.
--- bash: show command directly in code block
--- read_file/write_file: show path
--- others: show as key: value pairs
local function format_tool_call(part)
    local tool = part.tool or "?"
    local raw = part.args_brief or ""

    local ok, args = pcall(cru.json.decode, raw)
    if not ok then args = nil end

    if tool == "bash" and args and args.command then
        return "> \u{1f527} `bash` ```\n" .. args.command .. "\n```"
    end

    if (tool == "read_file" or tool == "write_file" or tool == "edit_file")
        and args and args.path then
        return "> \u{1f527} `" .. tool .. "` `" .. args.path .. "`"
    end

    if tool == "grep" and args then
        local pattern = args.pattern or args.query or ""
        local path = args.path or ""
        return "> \u{1f527} `grep` `" .. pattern .. "` in `" .. path .. "`"
    end

    if args then
        local parts = {}
        for k, v in pairs(args) do
            local val = type(v) == "string" and v or cru.json.encode(v)
            if #val > 80 then val = val:sub(1, 77) .. "..." end
            table.insert(parts, k .. ": " .. val)
        end
        if #parts > 0 then
            return "> \u{1f527} `" .. tool .. "`\n> " .. table.concat(parts, "\n> ")
        end
    end

    if #raw > 200 then raw = raw:sub(1, 197) .. "..." end
    return "> \u{1f527} `" .. tool .. "` " .. raw
end

local function format_tool_result(part)
    local icon = part.is_error and "\u{274c}" or "\u{2705}"
    local brief = part.result_brief or ""
    if #brief > 800 then brief = brief:sub(1, 797) .. "..." end
    if #brief > 100 then
        return "> " .. icon .. "\n```\n" .. brief .. "\n```"
    end
    return "> " .. icon .. " `" .. brief .. "`"
end

local function send_chunked(channel_id, text, reply_to_msg_id)
    local chunks = chunk_text(text)
    for i, chunk in ipairs(chunks) do
        local opts = {}
        if i == 1 and reply_to_msg_id then
            opts.reply_to = reply_to_msg_id
        end
        local _, send_err = api.send_message(channel_id, chunk, opts)
        if send_err then
            cru.log("warn", "Failed to send chunk: " .. tostring(send_err))
            return send_err
        end
    end
    return nil
end

--- Wait for a permission reply from the original user.
--- Returns {allowed=bool, scope=string, reason=string|nil} or nil on timeout.
local function wait_for_permission_reply(channel_id, user_id)
    M.pending_replies[channel_id] = { state = "waiting", user_id = user_id }
    local waited = 0
    while M.pending_replies[channel_id]
        and M.pending_replies[channel_id].state == "waiting"
        and waited < PERMISSION_TIMEOUT do
        cru.timer.sleep(0.5)
        waited = waited + 0.5
    end
    local pending = M.pending_replies[channel_id]
    M.pending_replies[channel_id] = nil
    if not pending or pending.state == "waiting" then return nil end
    return {
        allowed = pending.state == "allowed",
        scope = pending.scope or "once",
        reason = pending.reason,
    }
end

--- Format a permission request prompt for Discord.
local function format_permission_prompt(part)
    local desc = part.description or ""
    if #desc > 300 then desc = desc:sub(1, 297) .. "..." end
    return string.format(
        "> \u{26a0}\u{fe0f} **%s** wants to run:\n> ```\n> %s\n> ```\n> **y** / **n** / **y!** (allow session) / **n, reason**",
        part.tool or "unknown",
        desc
    )
end

--- Send a user message to a Crucible session and stream response parts to Discord.
---@param session_id string Crucible session ID
---@param channel_id string Discord channel ID
---@param user_message string The user's message content
---@param reply_to_msg_id string|nil Discord message ID to reply to
---@param user_id string|nil Discord user ID of the requester (for permission auth)
function M.respond(session_id, channel_id, user_message, reply_to_msg_id, user_id)
    cru.log("info", "Responder: starting for session " .. session_id)
    pcall(api.trigger_typing, channel_id)

    local next_part, err = cru.sessions.send_and_collect(session_id, user_message, {
        timeout = RESPONSE_TIMEOUT,
    })

    if err then
        cru.log("warn", "Responder: send_and_collect failed: " .. tostring(err))
        api.send_message(channel_id, "Sorry, I couldn't process that: " .. tostring(err), {
            reply_to = reply_to_msg_id,
        })
        return
    end

    local last_typing = 0
    local function next_part_with_typing()
        local now = cru.timer.clock()
        if now - last_typing > TYPING_INTERVAL then
            pcall(api.trigger_typing, channel_id)
            last_typing = now
        end
        return next_part()
    end

    local first_message = true
    local part_count = 0

    while true do
        local part = next_part_with_typing()
        if part == nil then break end
        part_count = part_count + 1

        local reply_id = first_message and reply_to_msg_id or nil

        if part.type == "text" then
            local content = tables.transform(part.content or "")
            if content ~= "" then
                send_chunked(channel_id, content, reply_id)
                first_message = false
            end

        elseif part.type == "tool_call" then
            local msg = format_tool_call(part)
            api.send_message(channel_id, msg, { reply_to = reply_id })
            first_message = false
            pcall(api.trigger_typing, channel_id)

        elseif part.type == "tool_result" then
            local msg = format_tool_result(part)
            api.send_message(channel_id, msg, { reply_to = reply_id })
            first_message = false

        elseif part.type == "thinking" then
            pcall(api.trigger_typing, channel_id)

        elseif part.type == "permission_request" then
            local prompt = format_permission_prompt(part)
            api.send_message(channel_id, prompt, { reply_to = reply_id })
            first_message = false

            local wait_ok, reply = pcall(wait_for_permission_reply, channel_id, user_id)
            if not wait_ok then
                M.pending_replies[channel_id] = nil
                reply = nil
            end
            if not reply then
                api.send_message(channel_id, "> \u{23f0} Permission timed out — denying.")
                reply = { allowed = false, scope = "once" }
            end

            local _, respond_err = cru.sessions.interaction_respond(
                session_id, part.request_id, reply
            )
            if respond_err then
                cru.log("warn", "Failed to respond to permission: " .. tostring(respond_err))
            end

            if reply.allowed then
                local note = reply.scope == "session" and " (session)" or ""
                api.send_message(channel_id, "> \u{2705} Approved" .. note)
            else
                local note = reply.reason and (": " .. reply.reason) or ""
                if reply.scope == "session" then note = " (all denied)" .. note end
                api.send_message(channel_id, "> \u{1f6ab} Denied" .. note)
            end
            pcall(api.trigger_typing, channel_id)
        end
    end

    if part_count == 0 then
        api.send_message(channel_id, "I didn't have a response for that.", {
            reply_to = reply_to_msg_id,
        })
    end

    cru.log("info", "Responder: done (" .. part_count .. " parts)")
end

return M
