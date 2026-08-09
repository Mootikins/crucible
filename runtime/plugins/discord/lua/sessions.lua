--- Discord channel-to-session mapping
--- Manages Crucible agent sessions per Discord channel.

local config = require("config")

local M = {}

-- channel_id -> { session_id, last_active, guild_id }
local channel_sessions = {}

-- Session inactivity timeouts (seconds)
local DM_SESSION_TTL      = 86400   -- 24 hours for DMs
local CHANNEL_SESSION_TTL = 900     -- 15 minutes for channel @mentions
local STALE_TTL           = 7200    -- 2 hours before ending idle sessions

--- Get the session TTL based on context (DM vs channel).
local function session_ttl(guild_id)
    if not guild_id then
        return DM_SESSION_TTL
    end
    return CHANNEL_SESSION_TTL
end

--- Get or create a Crucible session for a Discord channel.
--- Reuses an existing session if it was active within the TTL window.
function M.get_or_create(channel_id, guild_id)
    local entry = channel_sessions[channel_id]
    local ttl = session_ttl(guild_id)

    if entry then
        local age = os.time() - entry.last_active
        if age < ttl then
            entry.last_active = os.time()
            return entry.session_id, nil
        end
        -- Session too old, end it and create fresh
        pcall(cru.sessions.end_session, entry.session_id)
    end

    -- The kiln is required, not defaulted: `create_session` falls back to
    -- `crucible_home()` when it is absent, which stages reflection proposals
    -- under `~/.crucible/.crucible/proposals/` where `cru proposals list`
    -- never looks. Refuse before creating anything.
    local kiln = config.get("kiln")
    if not kiln then
        return nil, "Discord plugin: no kiln configured — set [plugins.discord] kiln in your Crucible config"
    end

    local create_opts = { type = "chat", kiln = kiln }

    -- Add configured read kilns if present
    local kilns = config.get("kilns")
    if kilns then
        create_opts.kilns = kilns
    end

    local session, err = cru.sessions.create(create_opts)
    if not session then
        return nil, "Failed to create session: " .. tostring(err)
    end

    -- A session whose agent could not be configured answers every message with
    -- "NoAgentConfigured" for the full TTL if it is cached, so end it and
    -- report the failure instead.
    if not M.configure_agent(session.id) then
        pcall(cru.sessions.end_session, session.id)
        return nil, "Discord plugin: could not configure an agent for this session"
    end

    channel_sessions[channel_id] = {
        session_id = session.id,
        last_active = os.time(),
        guild_id = guild_id,
    }

    return session.id, nil
end

--- Configure the agent for a session with optional overrides from plugin config.
--- Returns true when the session has a usable agent.
function M.configure_agent(session_id)
    local provider = config.get("provider")
    local model = config.get("model")

    if not provider or not model then
        cru.log("warn", "Discord plugin: provider and model must be configured")
        return false
    end

    local agent_config = {
        agent_type = config.get("agent_type", "internal"),
        provider = provider,
        model = model,
        system_prompt = config.get("system_prompt",
            "You are a knowledgeable assistant in a Discord chat. "
            .. "Conversations here are short — usually one or two exchanges — so make each response count. "
            .. "Be thorough and thoughtful rather than terse; the user may not follow up. "
            .. "Use Discord markdown formatting (bold, code blocks, lists) when it helps clarity."),
    }

    -- Optional fields
    local provider_key = config.get("provider_key")
    if provider_key then agent_config.provider_key = provider_key end

    local agent_name = config.get("agent_name")
    if agent_name then agent_config.agent_name = agent_name end

    local _, err = cru.sessions.configure_agent(session_id, agent_config)
    if err then
        cru.log("warn", "Failed to configure agent for session " .. session_id .. ": " .. tostring(err))
        return false
    end

    return true
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
