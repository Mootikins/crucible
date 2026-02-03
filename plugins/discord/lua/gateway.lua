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
local event_handlers = {}

-- ---------------------------------------------------------------------------
-- Internal helpers
-- ---------------------------------------------------------------------------

local function send_payload(op, d)
    if not ws then return end
    local payload = crucible.json_encode({ op = op, d = d })
    ws:send(payload)
end

local function send_heartbeat()
    send_payload(OP.HEARTBEAT, last_sequence)
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
-- Event dispatch
-- ---------------------------------------------------------------------------

--- Register a handler for a gateway event
function M.on(event_name, handler)
    if not event_handlers[event_name] then
        event_handlers[event_name] = {}
    end
    table.insert(event_handlers[event_name], handler)
end

local function dispatch_event(event_name, data)
    local handlers = event_handlers[event_name]
    if not handlers then return end
    for _, handler in ipairs(handlers) do
        local ok, err = pcall(handler, data)
        if not ok then
            crucible.log("warn", "Discord event handler error (" .. event_name .. "): " .. tostring(err))
        end
    end
end

-- ---------------------------------------------------------------------------
-- Message processing
-- ---------------------------------------------------------------------------

local function handle_message(raw)
    if not raw or raw.type ~= "text" then return true end

    local ok, msg = pcall(crucible.json_decode, raw.data)
    if not ok then
        crucible.log("warn", "Discord gateway: failed to decode message")
        return true
    end

    local op = msg.op

    -- Track sequence number
    if msg.s then
        last_sequence = msg.s
    end

    if op == OP.HELLO then
        heartbeat_interval = msg.d.heartbeat_interval
        if session_id then
            send_resume()
        else
            send_identify()
        end
        return true

    elseif op == OP.HEARTBEAT_ACK then
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
            crucible.log("info", "Discord gateway: connected as " .. msg.d.user.username)
        end

        dispatch_event(event_name, msg.d)
        return true

    elseif op == OP.RECONNECT then
        crucible.log("info", "Discord gateway: server requested reconnect")
        return false

    elseif op == OP.INVALID_SESSION then
        if msg.d then
            crucible.log("info", "Discord gateway: invalid session (resumable)")
            send_resume()
        else
            crucible.log("info", "Discord gateway: invalid session, re-identifying")
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

--- Connect to Discord Gateway and run receive loop.
--- Blocks the calling context. Returns on disconnect/error.
function M.connect()
    local url = resume_gateway_url or config.gateway_url()

    crucible.log("info", "Discord gateway: connecting to " .. url)
    ws = cru.ws.connect(url)

    if not ws then
        error("Failed to connect to Discord gateway")
    end

    -- Main receive loop
    local last_heartbeat = os.clock()

    while true do
        -- Send heartbeat if interval elapsed
        if heartbeat_interval then
            local now = os.clock()
            if (now - last_heartbeat) * 1000 >= heartbeat_interval then
                send_heartbeat()
                last_heartbeat = now
            end
        end

        local msg = ws:receive()
        if msg then
            local should_continue = handle_message(msg)
            if not should_continue then
                ws:close()
                ws = nil
                crucible.log("info", "Discord gateway: reconnecting...")
                return M.connect()
            end
        end
    end
end

--- Disconnect from gateway
function M.disconnect()
    if ws then
        is_connected = false
        session_id = nil
        last_sequence = nil
        ws:close()
        ws = nil
        crucible.log("info", "Discord gateway: disconnected")
    end
end

--- Check if currently connected
function M.is_connected()
    return is_connected
end

--- Get current session info
function M.session_info()
    return {
        connected = is_connected,
        session_id = session_id,
        last_sequence = last_sequence,
    }
end

return M
