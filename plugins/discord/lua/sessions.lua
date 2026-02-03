--- Discord channel-to-session mapping
--- Manages Crucible agent sessions per Discord channel.

local config = require("config")

local M = {}

-- channel_id -> { session_id, last_active, guild_id }
local channel_sessions = {}

-- Session inactivity timeout (seconds)
local SESSION_TTL = 3600      -- 1 hour before creating a new session
local STALE_TTL   = 7200      -- 2 hours before ending idle sessions

--- Get or create a Crucible session for a Discord channel.
--- Reuses an existing session if it was active within SESSION_TTL.
function M.get_or_create(channel_id, guild_id)
    local entry = channel_sessions[channel_id]
    if entry then
        local age = os.time() - entry.last_active
        if age < SESSION_TTL then
            entry.last_active = os.time()
            return entry.session_id, nil
        end
        -- Session too old, end it and create fresh
        pcall(cru.sessions.end_session, entry.session_id)
    end

    -- Create new session using the daemon's active kiln
    local kiln_path = cru.kiln.active_path
    if not kiln_path or kiln_path == "" then
        return nil, "No kiln is open in the daemon"
    end

    local session, err = cru.sessions.create("chat", kiln_path)
    if not session then
        return nil, "Failed to create session: " .. tostring(err)
    end

    -- Configure agent with defaults (provider/model come from daemon config)
    M.configure_agent(session.id)

    channel_sessions[channel_id] = {
        session_id = session.id,
        last_active = os.time(),
        guild_id = guild_id,
    }

    return session.id, nil
end

--- Configure the agent for a session with optional overrides from plugin config.
function M.configure_agent(session_id)
    local agent_config = {}
    for _, key in ipairs({ "provider", "model", "system_prompt" }) do
        local val = config.get(key)
        if val then agent_config[key] = val end
    end

    if not next(agent_config) then return end

    local _, err = cru.sessions.configure_agent(session_id, agent_config)
    if err then
        cru.log("warn", "Failed to configure agent for session " .. session_id .. ": " .. tostring(err))
    end
end

--- End and remove stale sessions (inactive > STALE_TTL).
function M.cleanup_stale()
    local now = os.time()
    local to_remove = {}

    for channel_id, entry in pairs(channel_sessions) do
        if now - entry.last_active > STALE_TTL then
            pcall(cru.sessions.end_session, entry.session_id)
            table.insert(to_remove, channel_id)
        end
    end

    for _, channel_id in ipairs(to_remove) do
        channel_sessions[channel_id] = nil
    end
end

--- Get current session count.
function M.active_count()
    local count = 0
    for _ in pairs(channel_sessions) do
        count = count + 1
    end
    return count
end

return M
