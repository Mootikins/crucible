--- Discord Gateway WebSocket client
--- Connects directly to Discord Gateway via cru.ws.connect()
--- Handles: Hello -> Identify -> Heartbeat loop -> Dispatch events

local config = require("config")

local M = {}

-- Gateway opcodes
local OP = {
    DISPATCH            = 0,
    HEARTBEAT           = 1,
    IDENTIFY            = 2,
    PRESENCE_UPDATE     = 3,
    VOICE_STATE_UPDATE  = 4,
    RESUME              = 6,
    RECONNECT           = 7,
    REQUEST_GUILD_MEMBERS = 8,
    INVALID_SESSION     = 9,
    HELLO               = 10,
    HEARTBEAT_ACK       = 11,
}

-- State
local ws = nil
local heartbeat_interval = nil
local last_sequence = nil
local session_id = nil
local resume_gateway_url = nil
local is_connected = false
local awaiting_ack = false
-- Whether the current `M.connect` cycle ever reached a live connection. Decides
-- whether an exhausted retry budget earns a fresh one or is a dead gateway.
local connected_this_cycle = false
-- Set by `M.disconnect`, cleared by `M.connect`. The retry predicate below is
-- the only thing separating a shutdown from a reconnect, because every error
-- the receive loop can raise is otherwise indistinguishable from an outage.
local stopped = false

-- Event system
local events = cru.emitter.new()

-- Periodic hook (called on each receive loop iteration / timeout)
local periodic_hook = nil

--- Register a handler for a gateway event (delegates to emitter)
function M.on(event_name, handler) return events:on(event_name, handler) end
function M.once(event_name, handler) return events:once(event_name, handler) end
function M.off(event_name, id) events:off(event_name, id) end

--- Set a function to be called periodically in the receive loop.
--- Used for digest delivery and session cleanup.
function M.set_periodic_hook(fn) periodic_hook = fn end

-- ---------------------------------------------------------------------------
-- Internal helpers
-- ---------------------------------------------------------------------------

local function send_payload(op, d)
    if not ws then return false end
    local ok, err = pcall(function()
        ws:send(cru.json.encode({ op = op, d = d }))
    end)
    if not ok then
        cru.log("warn", "Discord gateway: send failed: " .. tostring(err))
        return false
    end
    return true
end

local function send_heartbeat()
    -- A second heartbeat with the previous one unacknowledged means the socket
    -- is a zombie: Discord is gone but the TCP connection has not noticed.
    -- Logging and carrying on left the bot silently offline until the OS timed
    -- the socket out, which can be many minutes.
    if awaiting_ack then
        cru.log("warn", "Discord gateway: missed heartbeat ACK, dropping zombie connection")
        ws = nil
        error({ retryable = true })
    end
    awaiting_ack = true
    return send_payload(OP.HEARTBEAT, last_sequence)
end

local function send_identify()
    send_payload(OP.IDENTIFY, {
        token = config.get_token(),
        intents = config.get_intents(),
        properties = {
            os = "linux",
            browser = "crucible",
            device = "crucible",
        },
    })
end

local function send_resume()
    send_payload(OP.RESUME, {
        token = config.get_token(),
        session_id = session_id,
        seq = last_sequence,
    })
end

-- ---------------------------------------------------------------------------
-- Message processing
-- ---------------------------------------------------------------------------

local function handle_message(raw)
    if not raw or raw.type ~= "text" then return true end

    local ok, msg = pcall(cru.json.decode, raw.data)
    if not ok then
        cru.log("warn", "Discord gateway: failed to decode message")
        return true
    end

    local op = msg.op

    if msg.s then last_sequence = msg.s end

    if op == OP.HELLO then
        heartbeat_interval = msg.d.heartbeat_interval
        if session_id then
            send_resume()
        else
            send_identify()
        end
        return true

    elseif op == OP.HEARTBEAT_ACK then
        awaiting_ack = false
        return true

    elseif op == OP.HEARTBEAT then
        send_heartbeat()
        return true

    elseif op == OP.DISPATCH then
        local event_name = msg.t

        if event_name == "READY" then
            session_id = msg.d.session_id
            resume_gateway_url = msg.d.resume_gateway_url
            is_connected = true
            connected_this_cycle = true
            cru.log("info", "Discord gateway: connected as " .. msg.d.user.username)
        elseif event_name == "RESUMED" then
            -- A reconnect that resumes never sees READY, so this was the only
            -- dispatch that could mark the gateway up — and a working bot
            -- reported itself down, which `:discord status` then told the
            -- operator to "fix" by connecting a second time. RESUMED carries
            -- neither `session_id` nor `user`, so only the flag is set here.
            is_connected = true
            connected_this_cycle = true
            cru.log("info", "Discord gateway: resumed")
        end

        events:emit(event_name, msg.d)
        return true

    elseif op == OP.RECONNECT then
        cru.log("info", "Discord gateway: server requested reconnect")
        return false

    elseif op == OP.INVALID_SESSION then
        if msg.d then
            cru.log("info", "Discord gateway: invalid session (resumable)")
            send_resume()
        else
            cru.log("info", "Discord gateway: invalid session, re-identifying")
            session_id = nil
            last_sequence = nil
            send_identify()
        end
        return true
    end

    return true
end

-- ---------------------------------------------------------------------------
-- Connection lifecycle
-- ---------------------------------------------------------------------------

--- Connect to Discord Gateway with reconnection backoff.
--- Blocks the calling context. Returns on clean disconnect or exhausted retries.
function M.connect()
    stopped = false
    -- One `cru.retry` per outage, not one for the process. The body below never
    -- returns while a connection is healthy, so a single `cru.retry` spends its
    -- budget across the daemon's whole life: every drop ever — including
    -- Discord's routine `OP 7 RECONNECT` — permanently consumes a slot and the
    -- backoff never decays, so the eleventh drop raises and the service ends
    -- with nothing to restart it (`plugin_boot.rs` logs the failure and stops).
    -- Re-entering gives each outage a fresh ten attempts and a fresh backoff.
    --
    -- The budget resets on a *connection*, not unconditionally: a cycle that
    -- never reached READY or RESUMED means the gateway is genuinely
    -- unreachable, and re-entering on that would busy-spin through ten
    -- immediate failures forever. So a cycle that made no progress re-raises,
    -- which is the old behaviour and the correct one.
    while not stopped do
        connected_this_cycle = false
        local ok, err = pcall(M.connect_once)
        if stopped or ok then
            return
        end
        if not connected_this_cycle then
            error(err)
        end
        cru.log("info", "Discord gateway: retry budget spent after a live connection; starting a fresh one")
    end
end

--- One connection's worth of retries. Raises when the budget is spent.
function M.connect_once()
    cru.retry(function()
        -- `cru.retry` sleeps between attempts, so a disconnect can land while
        -- this body is waiting to be re-entered.
        if stopped then return end
        -- Every path out of the loop below raises, so this is the one place
        -- that can honestly say the gateway is down. `awaiting_ack` belongs
        -- here for the same reason: it is per-connection, and the zombie drop
        -- that sets it leaves `ws` nil, so `M.disconnect` never gets to clear
        -- it. Carried into the next attempt it dropped a healthy socket at its
        -- first heartbeat, and every attempt after that, until the budget was
        -- gone.
        is_connected = false
        awaiting_ack = false

        local url = resume_gateway_url or config.gateway_url()
        cru.log("info", "Discord gateway: connecting to " .. url)

        ws = cru.ws.connect(url)

        -- Receive loop with explicit heartbeat tracking
        -- Initial heartbeat uses jitter (random fraction of interval) per Discord spec
        local last_heartbeat_at = cru.timer.clock()
        local first_heartbeat = true

        while true do
            -- Compute time until next heartbeat is due
            local recv_timeout = 30.0
            if heartbeat_interval then
                local interval_secs = heartbeat_interval / 1000.0
                local elapsed = cru.timer.clock() - last_heartbeat_at
                -- First heartbeat uses jitter: random 0..interval per Discord spec
                local target = first_heartbeat and (interval_secs * math.random()) or interval_secs
                local remaining = target - elapsed
                if remaining <= 0 then
                    if not send_heartbeat() then
                        -- WebSocket closed during heartbeat, trigger reconnect
                        ws = nil
                        error({ retryable = true })
                    end
                    last_heartbeat_at = cru.timer.clock()
                    first_heartbeat = false
                    remaining = interval_secs
                end
                recv_timeout = remaining
            end

            local ok, msg = pcall(ws.receive, ws, recv_timeout)

            if not ok then
                -- ws:receive threw an error (connection closed, etc.)
                cru.log("info", "Discord gateway: receive error: " .. tostring(msg))
                ws = nil
                error({ retryable = true })
            end

            if msg then
                local hok, should_continue = pcall(handle_message, msg)
                if not hok or not should_continue then
                    pcall(function() ws:close() end)
                    ws = nil
                    error({ retryable = true })
                end
            end
            -- msg == nil means timeout, loop continues and heartbeat fires at top

            -- Run periodic hook (digest, session cleanup, etc.)
            if periodic_hook then pcall(periodic_hook) end
            -- On timeout, loop continues and heartbeat fires at top
        end
    end, {
        max_retries = 10,
        base_delay = 1.0,
        max_delay = 60.0,
        -- `cru.ws.connect` raises a string, not a table, so a predicate keyed on
        -- `err.retryable` rejected every real network failure and re-raised on
        -- the first one — ten attempts that never happened. Retry on anything
        -- and let `disconnect` be the only thing that stops us; deleting the
        -- predicate instead would not do, because the stdlib default accepts
        -- everything and a disconnected `ws` raises on the next `ws.receive`
        -- lookup, turning a deliberate shutdown into a reconnect.
        retryable = function() return not stopped end,
    })
end

--- Update bot presence/status.
---@param status string "online"|"idle"|"dnd"|"invisible"
---@param activity table|nil {name=string, type=number} (0=Playing, 1=Streaming, 2=Listening, 3=Watching, 5=Competing)
function M.update_presence(status, activity)
    -- Discord requires "since" as null or int, but Lua nil omits the key.
    -- Encode manually to guarantee correct JSON structure.
    local activities_json = "[]"
    if activity then
        activities_json = cru.json.encode({ activity })
    end
    local since = "null"
    if status == "idle" then
        since = tostring(math.floor(os.time() * 1000))
    end
    local json = string.format(
        '{"op":3,"d":{"since":%s,"activities":%s,"status":"%s","afk":%s}}',
        since, activities_json, status, status == "idle" and "true" or "false"
    )
    if ws then
        pcall(function() ws:send(json) end)
    end
end

--- Disconnect from gateway (clean disconnect clears session)
function M.disconnect()
    -- Before the close, and outside the `ws` guard: during an outage `ws` is
    -- already nil, and that is exactly when a caller most needs the retry loop
    -- to stop rather than to dial again.
    stopped = true
    -- Outside the `ws` guard, not inside it: during an outage `ws` is already
    -- nil, and that is precisely when the state must be cleared. Left inside,
    -- a disconnect mid-outage returned with `is_connected` still true, so
    -- `:discord status` claimed a live connection and `:discord connect`
    -- refused with "Already connected" until the daemon was restarted.
    is_connected = false
    session_id = nil
    last_sequence = nil
    resume_gateway_url = nil
    awaiting_ack = false
    if ws then
        ws:close()
        ws = nil
    end
    cru.log("info", "Discord gateway: disconnected")
end

function M.is_connected() return is_connected end

function M.session_info()
    return {
        connected = is_connected,
        session_id = session_id,
        last_sequence = last_sequence,
    }
end

return M
