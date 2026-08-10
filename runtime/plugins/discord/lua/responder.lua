--- Discord agent response collection and delivery
--- Routes messages to Crucible sessions and streams response parts back to Discord.

local api = require("api")
local sessions = require("sessions")

local M = {}

local MAX_MESSAGE_LEN = 2000
local RESPONSE_TIMEOUT = 120  -- seconds
local TYPING_INTERVAL = 8    -- seconds between typing indicator refreshes

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

--- Send a user message to a Crucible session and stream response parts to Discord.
---@param session_id string Crucible session ID
--- Seconds to wait for a y/n reply before denying.
local PERMISSION_TIMEOUT = 60

--- channel_id -> { state = "waiting"|"allowed"|"denied", user_id = string }
---
--- Keyed by the channel the *prompt* was shown in — the approver's DM when
--- approval is delegated, the requesting channel when the requester answers
--- their own. `user_id` is the one account entitled to answer it, which is what
--- makes the key sound: a room may hold many people, but the prompt in it
--- belongs to one of them.
---
--- At most one entry per channel, ever: see `reserve_pending`.
M.pending_replies = {}

--- Claim the one outstanding-prompt slot for `channel_id`, or return nil
--- because another request already holds it.
---
--- A verdict is a bare "y" carrying nothing that ties it to a request, so it
--- can only be read against a single outstanding prompt. Delegation is what
--- makes two of them reachable: every requester's prompt lands in the same
--- approver DM however many rooms they came from, and dropping the DM-only
--- restriction lets two speakers in one room prompt in place. Sharing a slot
--- would mean an approval granted for one tool call resolving another, from a
--- different account — so the second request is refused here, before anything
--- is shown, and denied. Queueing it instead would leave the approver answering
--- prompts they can no longer see, which is the same ambiguity deferred.
local function reserve_pending(channel_id, user_id)
    if M.pending_replies[channel_id] then return nil end
    local pending = { state = "waiting", user_id = user_id }
    M.pending_replies[channel_id] = pending
    return pending
end

--- Release a slot, but only if it is still the one we claimed.
local function release_pending(channel_id, pending)
    if M.pending_replies[channel_id] == pending then
        M.pending_replies[channel_id] = nil
    end
end

--- Consume a message that answers an outstanding permission prompt.
---
--- Returns true when it was an answer, in which case the caller must not route
--- it as a turn — it is a verdict, not a question, and must cost no quota. The
--- authorization is the author id: only the account the prompt named resolves
--- it, so a bystander typing "y" in the same channel is an ordinary message.
function M.try_resolve_permission(channel_id, author_id, content)
    local pending = M.pending_replies[channel_id]
    if not pending or pending.state ~= "waiting" then return false end
    if not author_id or author_id ~= pending.user_id then return false end

    local verdict, reason = (content or ""):lower():match("^(%a+)%s*,?%s*(.*)$")
    if verdict == "y" or verdict == "yes" then
        pending.state = "allowed"
        return true
    elseif verdict == "n" or verdict == "no" then
        pending.state = "denied"
        pending.reason = reason ~= "" and reason or nil
        return true
    end
    return false
end

--- Block until the named account answers `pending`, or the timeout expires.
---
--- Waits on the entry it was handed rather than on whatever the channel's slot
--- holds when it wakes: a verdict belongs to the request it was given for, and
--- reading the slot back is how it would come to belong to another one.
local function wait_for_permission_reply(channel_id, pending)
    local waited = 0
    while pending.state == "waiting" and waited < PERMISSION_TIMEOUT do
        cru.timer.sleep(0.5)
        waited = waited + 0.5
    end
    release_pending(channel_id, pending)
    if pending.state == "waiting" then return nil end
    return { allowed = pending.state == "allowed", reason = pending.reason }
end

--- The prompt itself: a callout naming the tool, with the arguments quoted.
---
--- `requester_id` and `from_channel_id` are set only when the prompt is going
--- somewhere the requester is not — the approver is deciding on behalf of an
--- account they cannot otherwise see, and "who is asking, and from where" is
--- half of that decision. Rendered as mentions, so Discord resolves them to a
--- name and a channel for the reader.
---
--- No "allow for the session" option. The previous one sent `scope = "session"`
--- with no pattern, and `permission.rs` honours a scope only for `Project` and
--- only alongside a pattern — so it granted nothing while reporting success.
--- Standing permission is what the `write` tier is for; this prompt answers one
--- call.
local function format_permission_prompt(part, requester_id, from_channel_id)
    local desc = part.description or ""
    if #desc > 300 then desc = desc:sub(1, 297) .. "..." end
    local asked_by = ""
    if requester_id then
        asked_by = string.format("> <@%s> in <#%s> asked.\n",
            tostring(requester_id), tostring(from_channel_id))
    end
    return asked_by .. string.format(
        "> \u{26a0}\u{fe0f} **%s** wants to run:\n> ```\n> %s\n> ```\n> Reply **y** to allow, **n** to deny (optionally `n, reason`).",
        part.tool or "unknown",
        desc
    )
end

--- Where a permission prompt goes and who may answer it.
---
--- With `approvers` configured the prompt goes to the first approver's DM, so
--- the request itself may come from anywhere: the room never sees the prompt
--- and cannot answer it. That is the fix for the requester approving their own
--- request — a chat-room username is not a Crucible principal, but an account
--- the operator named in config is.
---
--- With none configured the requester answers in place, which
--- `sessions.access_tier` only offers to someone a `user:` or `role:` key
--- named. Putting your own id in `approvers` gives the personal case with no
--- special path through here: you are both parties.
---
--- Returns nil and a reason when the approver cannot be reached, which denies.
--- Prompting the requester instead would be a fallback to the weaker rule the
--- approver list exists to replace.
local function permission_target(channel_id, requester_id)
    local approver = sessions.approvers()[1]
    if not approver then
        return { channel_id = channel_id, user_id = requester_id, delegated = false }
    end

    -- A pcall because `api_request` *raises* when `cru.retry` runs out of
    -- attempts, and an unanswered permission request would otherwise abandon
    -- the turn mid-flight instead of denying one tool call.
    local ok, dm, err = pcall(api.create_dm_channel, approver)
    if not ok then return nil, tostring(dm) end
    if not dm or not dm.id then
        return nil, err or "no DM channel returned"
    end
    return { channel_id = dm.id, user_id = tostring(approver), delegated = true }
end

--- Tell the requester something, never at the cost of the verdict.
---
--- Every send here is a pcall because `api_request` *raises* on an exhausted
--- retry budget, and one raised on the way out of `ask_permission` would
--- abandon the turn with the daemon's request still outstanding — worse than
--- the notice going unsent.
local function notify(channel_id, text, reply_to)
    pcall(api.send_message, channel_id, text, { reply_to = reply_to })
end

--- Ask the target for a verdict and return it, denying on a timeout or a
--- failure to reach the approver. Never raises and never returns nil: an
--- unanswered prompt is a denial, and so is one that could not be shown.
local function ask_permission(part, channel_id, requester_id, reply_to)
    local target, target_err = permission_target(channel_id, requester_id)
    if not target then
        cru.log("warn", "Discord plugin: could not open an approver DM: " .. tostring(target_err))
        notify(channel_id, "> \u{26a0}\u{fe0f} Couldn't reach an approver — denying.", reply_to)
        return { allowed = false, reason = "no approver could be reached" }
    end

    local pending = reserve_pending(target.channel_id, target.user_id)
    if not pending then
        notify(channel_id,
            "> \u{26a0}\u{fe0f} Another request is already waiting on an answer — denying this one. Try again in a moment.",
            reply_to)
        return { allowed = false, reason = "another request was already awaiting an answer" }
    end

    -- A prompt that raised on the way out would strand this channel's slot as
    -- well as the turn, so nothing could ever prompt in it again.
    local shown = pcall(api.send_message, target.channel_id,
        format_permission_prompt(part,
            target.delegated and requester_id or nil,
            target.delegated and channel_id or nil),
        { reply_to = not target.delegated and reply_to or nil })
    if not shown then
        release_pending(target.channel_id, pending)
        cru.log("warn", "Discord plugin: the permission prompt could not be sent")
        notify(channel_id, "> \u{26a0}\u{fe0f} Couldn't reach an approver — denying.", reply_to)
        return { allowed = false, reason = "the prompt could not be shown" }
    end

    if target.delegated then
        notify(channel_id,
            "> \u{23f3} That needs approval — I've asked. I'll answer once they reply.",
            reply_to)
    end

    local ok, reply = pcall(wait_for_permission_reply, target.channel_id, pending)
    if not ok then
        release_pending(target.channel_id, pending)
        reply = nil
    end
    if not reply then
        -- To whoever asked, not to the approver: the requester is the one
        -- waiting on an answer.
        notify(channel_id, "> \u{23f0} No answer — denying.")
        reply = { allowed = false }
    end
    return reply
end

---@param channel_id string Discord channel ID
---@param user_message string The user's message content
---@param reply_to_msg_id string|nil Discord message ID to reply to
---@param user_id string|nil Discord user ID of the requester (for permission auth)
function M.respond(session_id, channel_id, user_message, reply_to_msg_id, user_id, interactive)
    cru.log("info", "Responder: starting for session " .. session_id)
    pcall(api.trigger_typing, channel_id)

    local next_part, err = cru.sessions.send_and_collect(session_id, user_message, {
        timeout = RESPONSE_TIMEOUT,
        -- Only the `ask` tier. The daemon cannot tell that a named account is
        -- standing by to answer a prompt; the plugin can — either an approver
        -- it will DM, or a requester a `user:`/`role:` key named — and
        -- asserting it here is what makes a prompt answerable at all.
        interactive = interactive or false,
    })

    if err then
        cru.log("warn", "Responder: send_and_collect failed: " .. tostring(err))
        -- One task is spawned per message against one session id, and
        -- `agent_manager` rejects a second concurrent turn before spawning
        -- anything (`messaging/send.rs`). That rejection is what keeps replies
        -- from interleaving, so it is normal operation, not a fault: say so
        -- rather than pasting the daemon's error and the session id into a
        -- public channel. No queue — a FIFO would add unbounded state and a new
        -- failure mode for a cosmetic problem.
        local text = tostring(err):find("Concurrent request in progress", 1, true)
            and "I'm still working on your previous message."
            or "Sorry, I couldn't process that: " .. tostring(err)
        api.send_message(channel_id, text, { reply_to = reply_to_msg_id })
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
            local content = part.content or ""
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

        elseif part.type == "permission_request" then
            local reply = ask_permission(part, channel_id, user_id, reply_id)
            first_message = false

            local _, respond_err =
                cru.sessions.interaction_respond(session_id, part.request_id, reply)
            if respond_err then
                cru.log("warn", "Failed to respond to permission: " .. tostring(respond_err))
            end
            pcall(api.trigger_typing, channel_id)

        elseif part.type == "thinking" then
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
