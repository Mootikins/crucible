--- Who answers a permission prompt, and where it is shown.
---
--- The `ask` tier used to post the prompt in the channel the request came from
--- and accept the requester's own "y" — a narrowed version of the hole
--- `e6626ad87` closed, because a chat-room username is not a Crucible
--- principal. With `approvers` configured the prompt goes to a named account's
--- DM instead, and only that account's reply resolves it; the requester is
--- told their request is waiting and nothing else.

local responder = require("responder")
local api = require("api")

--- Run `fn` with config, the session bridge, the clock and the REST surface
--- under the test's control, restoring all of them however `fn` ends.
---
--- `crucible.config` and `cru.sessions` are absent in the test VM. `api` is
--- stubbed by replacing fields on the module table, because `responder`
--- captured that table at load and it is what the stub has to reach.
local function with_env(cfg, env, fn)
    crucible = crucible or {}
    local had_config = crucible.config
    local had_sessions = cru.sessions
    local had_timer = cru.timer
    local had_send = api.send_message
    local had_typing = api.trigger_typing
    local had_dm = api.create_dm_channel

    crucible.config = { get = function(key) return cfg[key] end }
    cru.sessions = env.sessions
    -- A fixed clock keeps the typing refresh out of the way; `sleep` is where
    -- the test stands in for the reply arriving over the gateway.
    cru.timer = { clock = function() return 0 end, sleep = env.sleep or function() end }
    api.send_message = env.send_message
    api.trigger_typing = function() return {}, nil end
    api.create_dm_channel = env.create_dm_channel
        or function() error("no approver DM should be opened here") end

    local ok, err = pcall(fn)

    api.create_dm_channel = had_dm
    api.trigger_typing = had_typing
    api.send_message = had_send
    cru.timer = had_timer
    cru.sessions = had_sessions
    crucible.config = had_config
    if not ok then error(err) end
end

--- A session bridge whose one and only part is a permission request, recording
--- the verdict it is eventually answered with. `request_id` names the request
--- so a test with two turns in flight can tell their answers apart.
local function permission_session(answers, request_id)
    local parts = { {
        type = "permission_request",
        request_id = request_id or "req-1",
        tool = "write_file",
        description = "docs/Notes/scratch.md",
    } }
    return {
        send_and_collect = function()
            local i = 0
            return function()
                i = i + 1
                return parts[i]
            end, nil
        end,
        interaction_respond = function(_, request_id, reply)
            table.insert(answers, { request_id = request_id, reply = reply })
            return true, nil
        end,
    }
end

--- Records every message the bot sends, in order, with the channel it went to.
local function recorder()
    local sent = {}
    return sent, function(channel_id, content, opts)
        table.insert(sent, { channel = channel_id, text = content, opts = opts })
        return {}, nil
    end
end

local function is_prompt(entry)
    return entry.text:find("wants to run", 1, true) ~= nil
end

local function prompts_to(sent, channel_id)
    local n = 0
    for _, entry in ipairs(sent) do
        if entry.channel == channel_id and is_prompt(entry) then n = n + 1 end
    end
    return n
end

--- Run a second turn from a different account while the first is waiting, with
--- its own request id so the two verdicts are distinguishable.
local function second_turn(answers, channel_id, author_id)
    local outer = cru.sessions
    cru.sessions = permission_session(answers, "req-second")
    responder.respond("chat-2", channel_id, "write it too", "msg-2", author_id, true)
    cru.sessions = outer
end

describe("delegated approval", function()
    it("prompts the approver in a DM and honours their answer", function()
        local answers, dm_for, resolved = {}, {}, nil
        local sent, send = recorder()

        with_env({ ["discord.approvers"] = { "approver-1" } }, {
            sessions = permission_session(answers),
            send_message = send,
            create_dm_channel = function(user_id)
                table.insert(dm_for, user_id)
                return { id = "dm-approver" }, nil
            end,
            sleep = function()
                if resolved == nil then
                    resolved = responder.try_resolve_permission("dm-approver", "approver-1", "y")
                end
            end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        assert.deep_equal({ "approver-1" }, dm_for)

        -- The prompt goes to the approver's DM, and the room the request came
        -- from is told only that it is waiting.
        assert.equals("dm-approver", sent[1].channel)
        assert.truthy(is_prompt(sent[1]))
        assert.truthy(sent[1].text:find("write_file", 1, true))
        -- Whose request it is, and from where: an approver cannot authorize a
        -- tool call they are not told the requester for.
        assert.truthy(sent[1].text:find("requester-7", 1, true))
        assert.truthy(sent[1].text:find("room-9", 1, true))
        assert.equals("room-9", sent[2].channel)
        assert.falsy(is_prompt(sent[2]))

        assert.equals(true, resolved)
        assert.equals(1, #answers)
        assert.equals("req-1", answers[1].request_id)
        assert.equals(true, answers[1].reply.allowed)
    end)

    it("ignores the requester's own y and denies when the approver stays quiet", function()
        local answers, attempt, keys = {}, nil, nil
        local sent, send = recorder()

        with_env({ ["discord.approvers"] = { "approver-1" } }, {
            sessions = permission_session(answers),
            send_message = send,
            create_dm_channel = function() return { id = "dm-approver" }, nil end,
            sleep = function()
                if attempt == nil then
                    attempt = responder.try_resolve_permission("dm-approver", "requester-7", "y")
                    -- Snapshot rather than assert: `ask_permission` wraps the
                    -- wait in a pcall, so a raise in here would be swallowed
                    -- and read as the denial this test already expects.
                    keys = {
                        room = responder.pending_replies["room-9"],
                        dm = responder.pending_replies["dm-approver"],
                    }
                end
            end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        assert.equals(false, attempt)
        -- The outstanding prompt is keyed by the channel it was shown in,
        -- which is the approver's DM and not the room.
        assert.is_not_nil(keys)
        assert.is_nil(keys.room)
        assert.is_not_nil(keys.dm)
        assert.equals(1, #answers)
        assert.equals(false, answers[1].reply.allowed)

        -- The timeout is reported to whoever asked, not to the approver.
        local last = sent[#sent]
        assert.equals("room-9", last.channel)
        assert.truthy(last.text:find("No answer", 1, true))
    end)

    -- Falling back to the requester would be a fallback to the weaker rule the
    -- approver list exists to replace, so an unreachable approver denies.
    -- Both shapes of failure: `api_request` returns `(nil, err)` on a 4xx and
    -- *raises* when `cru.retry` runs out of attempts.
    local function denies_when_dm_fails(open_dm)
        local answers = {}
        local sent, send = recorder()

        with_env({ ["discord.approvers"] = { "approver-1" } }, {
            sessions = permission_session(answers),
            send_message = send,
            create_dm_channel = open_dm,
            sleep = function() error("nothing should wait on a prompt that was never sent") end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        for _, entry in ipairs(sent) do
            assert.falsy(is_prompt(entry))
        end
        assert.equals(1, #answers)
        assert.equals(false, answers[1].reply.allowed)
    end

    it("denies without prompting when the approver's DM is refused", function()
        denies_when_dm_fails(function() return nil, "Discord API error 403" end)
    end)

    it("denies without prompting when opening the approver's DM raises", function()
        denies_when_dm_fails(function() error("connection reset") end)
    end)

    it("denies and frees the slot when the prompt itself cannot be sent", function()
        local answers, sent = {}, {}

        with_env({ ["discord.approvers"] = { "approver-1" } }, {
            sessions = permission_session(answers),
            -- `api_request` raises when `cru.retry` runs out of attempts, and
            -- an approver with DMs closed is where that happens.
            send_message = function(channel_id, content, opts)
                if channel_id == "dm-approver" then error("connection reset") end
                table.insert(sent, { channel = channel_id, text = content, opts = opts })
                return {}, nil
            end,
            create_dm_channel = function() return { id = "dm-approver" }, nil end,
            sleep = function() error("nothing should wait on a prompt that was never shown") end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        -- The turn is answered rather than abandoned: a raise here used to
        -- escape `respond`, leaving the daemon's request outstanding forever.
        assert.equals(1, #answers)
        assert.equals(false, answers[1].reply.allowed)
        assert.equals("room-9", sent[1].channel)
        -- And the slot it reserved is released, or that channel could never
        -- carry another prompt.
        assert.is_nil(responder.pending_replies["dm-approver"])
    end)
end)

-- A verdict is a bare "y" with nothing tying it to a request, so it can only
-- be read against one outstanding prompt. Delegation is what makes two
-- reachable: every requester's prompt lands in the same approver DM, however
-- many rooms they came from. A second prompt there is refused rather than
-- queued or overwritten — an answer that could mean either request is an
-- approval granted for one tool call and spent on another.
describe("one prompt at a time", function()
    it("refuses a second request while the approver's DM already holds one", function()
        local answers, started, resolved = {}, false, nil
        local sent, send = recorder()

        with_env({ ["discord.approvers"] = { "approver-1" } }, {
            sessions = permission_session(answers),
            send_message = send,
            create_dm_channel = function() return { id = "dm-approver" }, nil end,
            sleep = function()
                if not started then
                    started = true
                    -- Another account, in another room, mid-wait.
                    second_turn(answers, "room-8", "requester-8")
                elseif resolved == nil then
                    resolved = responder.try_resolve_permission("dm-approver", "approver-1", "y")
                end
            end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        assert.equals(1, prompts_to(sent, "dm-approver"))
        assert.equals(true, resolved)

        -- The one "y" resolves the one prompt it was shown for, and nothing
        -- else. The refused request is denied, in its own room.
        assert.equals(2, #answers)
        assert.equals("req-second", answers[1].request_id)
        assert.equals(false, answers[1].reply.allowed)
        assert.equals("req-1", answers[2].request_id)
        assert.equals(true, answers[2].reply.allowed)
    end)

    it("refuses a second request while a room already holds a self-approval prompt", function()
        local answers, started, resolved = {}, false, nil
        local sent, send = recorder()

        with_env({}, {
            sessions = permission_session(answers),
            send_message = send,
            sleep = function()
                if not started then
                    started = true
                    -- A second `user:`-granted speaker in the same room, which
                    -- the retired DM-only restriction used to make impossible.
                    second_turn(answers, "room-9", "requester-8")
                elseif resolved == nil then
                    resolved = responder.try_resolve_permission("room-9", "requester-7", "y")
                end
            end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        assert.equals(1, prompts_to(sent, "room-9"))
        assert.equals(true, resolved)

        assert.equals(2, #answers)
        assert.equals("req-second", answers[1].request_id)
        assert.equals(false, answers[1].reply.allowed)
        assert.equals("req-1", answers[2].request_id)
        assert.equals(true, answers[2].reply.allowed)
    end)
end)

-- With no `approvers` the requester answers their own prompt in place, which
-- `sessions.access_tier` only hands out to someone a `user:` or `role:` key
-- named. Putting your own id in `approvers` is the same thing by the delegated
-- path: you are both parties.
describe("self approval", function()
    it("prompts in place and takes the requester's own answer", function()
        local answers = {}
        local sent, send = recorder()

        with_env({}, {
            sessions = permission_session(answers),
            send_message = send,
            sleep = function()
                responder.try_resolve_permission("room-9", "requester-7", "n, not that file")
            end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        assert.equals("room-9", sent[1].channel)
        assert.truthy(is_prompt(sent[1]))
        assert.equals(1, #answers)
        assert.equals(false, answers[1].reply.allowed)
        assert.equals("not that file", answers[1].reply.reason)
    end)

    it("does not let a bystander answer for the requester", function()
        local answers, attempt = {}, nil
        local sent, send = recorder()

        with_env({}, {
            sessions = permission_session(answers),
            send_message = send,
            sleep = function()
                if attempt == nil then
                    attempt = responder.try_resolve_permission("room-9", "bystander-2", "y")
                end
            end,
        }, function()
            responder.respond("chat-1", "room-9", "write it", "msg-1", "requester-7", true)
        end)

        assert.equals(false, attempt)
        assert.equals(false, answers[1].reply.allowed)
    end)
end)

describe("responder error disclosure", function()
    -- The fallback pasted `tostring(err)` into the channel. A daemon error
    -- carries absolute paths, session ids and provider text, and a Discord
    -- channel is read by everyone in the room -- including, on a shared server,
    -- people the session's own tier would never have let near it.
    local SECRET = "/home/moot/.crucible/sessions/abc-123/transcript.jsonl"

    local function failing_session(message)
        return {
            send_and_collect = function() return nil, message end,
            interaction_respond = function() return true, nil end,
        }
    end

    it("does not paste the daemon's error into the channel", function()
        local sent, record = recorder()
        with_env({}, {
            sessions = failing_session("stream failed reading " .. SECRET),
            send_message = record,
        }, function()
            responder.respond("chat-1", "room-9", "hi", "msg-1", "user-1", false)
        end)

        assert.equals(1, #sent)
        assert.equals(nil, sent[1].text:find(SECRET, 1, true))
        assert.equals(nil, sent[1].text:find("transcript", 1, true))
    end)

    it("still tells the user something went wrong", function()
        local sent, record = recorder()
        with_env({}, {
            sessions = failing_session("stream failed reading " .. SECRET),
            send_message = record,
        }, function()
            responder.respond("chat-1", "room-9", "hi", "msg-1", "user-1", false)
        end)

        assert.equals(true, #sent[1].text > 0)
    end)

    it("keeps the concurrent-request message, which is normal operation", function()
        local sent, record = recorder()
        with_env({}, {
            sessions = failing_session("Concurrent request in progress"),
            send_message = record,
        }, function()
            responder.respond("chat-1", "room-9", "hi", "msg-1", "user-1", false)
        end)

        assert.equals(true, sent[1].text:find("previous message", 1, true) ~= nil)
    end)
end)
