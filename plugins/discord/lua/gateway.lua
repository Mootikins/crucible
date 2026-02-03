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
    if awaiting_ack then
        cru.log("warn", "Discord gateway: missed heartbeat ACK, connection may be zombie")
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
            cru.log("info", "Discord gateway: connected as " .. msg.d.user.username)
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
    cru.retry(function()
        local url = resume_gateway_url or config.gateway_url()
        cru.log("info", "Discord gateway: connecting to " .. url)

        ws = cru.ws.connect(url)
        if not ws then error({ retryable = true }) end

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
        retryable = function(err)
            return type(err) == "table" and err.retryable
        end,
    })
end

--- Disconnect from gateway (clean disconnect clears session)
function M.disconnect()
    if ws then
        is_connected = false
        session_id = nil
        last_sequence = nil
        resume_gateway_url = nil
        awaiting_ack = false
        ws:close()
        ws = nil
        cru.log("info", "Discord gateway: disconnected")
    end
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
