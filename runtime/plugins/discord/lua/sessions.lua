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
function M.get_or_create(channel_id, guild_id, author_id)
    local entry = channel_sessions[channel_id]
    local ttl = session_ttl(guild_id)

    if entry then
        local age = os.time() - entry.last_active
        if age < ttl then
            entry.last_active = os.time()
            return entry.session_id, nil
        end
        -- Session too old, end it and create fresh. Drop the map entry with
        -- it: the only other place that clears one is the successful-create
        -- overwrite below, which the three error returns skip — so on a
        -- misconfigured daemon every later message in this channel re-ended
        -- the same dead id (swallowed by the pcall) and `active_count` kept
        -- reporting it to `:discord status`.
        pcall(cru.sessions.end_session, entry.session_id)
        channel_sessions[channel_id] = nil
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
    if not M.configure_agent(session.id, M.access_tier(guild_id, author_id)) then
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

--- What a Discord turn may do, by access tier.
---
--- A plugin turn is non-interactive, which the permission engine turns into
--- `Ask` -> `Deny`. That answers "who approves this?" — a chat-room username is
--- not a Crucible principal — but it is not the answer to "what may this
--- session do?". The plugin configures its own sessions, and these tables are
--- that configuration: `allow` runs with no prompt on the internal and the ACP
--- path alike.
---
--- Two tiers, because one bot instance serves people the operator trusts
--- differently. Reads are bounded by `allowed_roots` — the session kiln, its
--- connected kilns and the session dir — so "read" means *within the kilns you
--- configured*, not the filesystem.
---
--- Anything absent from a tier keeps default behaviour: `is_safe` reads pass,
--- everything else reaches the gate and is denied for want of an approver. Note
--- `allow` skips the gate entirely — mode stance, Lua `on_request` hooks and
--- saved patterns included — so both tables stay deliberate.
local READ_TOOLS = {
    read_file = "allow",
    glob = "allow",
    grep = "allow",
    read_note = "allow",
    list_notes = "allow",
    semantic_search = "allow",
    text_search = "allow",
}

--- The read set plus the tools that change the kiln. Still no `bash`: its blast
--- radius is not bounded by `allowed_roots`, so it stays a deliberate opt-in
--- via `[plugins.discord] tool_policy` rather than riding along with "write".
local WRITE_TOOLS = {}
for tool, policy in pairs(READ_TOOLS) do WRITE_TOOLS[tool] = policy end
WRITE_TOOLS.write_file = "allow"
WRITE_TOOLS.edit_file = "allow"
WRITE_TOOLS.multi_edit = "allow"
WRITE_TOOLS.create_note = "allow"
WRITE_TOOLS.update_note = "allow"

--- The read set, plus the write tools marked `ask` so the agent must get a
--- yes before each one.
---
--- Only meaningful in a DM: a prompt is answered by whoever replies first, and
--- the daemon keys permissions on `(session_id, permission_id)` alone, so in a
--- guild channel anyone present could answer for everyone. `M.access_tier`
--- degrades this to `read` outside a DM rather than trusting the room.
local ASK_TOOLS = {}
for tool, policy in pairs(WRITE_TOOLS) do
    ASK_TOOLS[tool] = READ_TOOLS[tool] and policy or "ask"
end

local TIERS = { read = READ_TOOLS, write = WRITE_TOOLS, ask = ASK_TOOLS }

--- Whether a tier needs a live person to answer prompts.
function M.tier_is_interactive(tier) return tier == "ask" end

--- The access tier for whoever triggered this turn.
---
--- Keyed on the two identities Discord actually gives us: the guild a message
--- came from, or the account that sent it. A DM channel is per-account, so both
--- are stable for the life of a channel session — which matters, because the
--- agent config is fixed when the session is created and every later message in
--- that channel reuses it.
---
---     [plugins.discord.access]
---     "user:1234" = "write"     # the operator's own DMs
---     "guild:5678" = "read"     # a server that may look but not touch
---     default = "read"
function M.access_tier(guild_id, author_id)
    local access = config.get("access", {})
    if type(access) ~= "table" then return "read" end

    local key = guild_id and ("guild:" .. tostring(guild_id))
        or (author_id and ("user:" .. tostring(author_id)))
    local tier = key and access[key] or access.default or "read"
    if not TIERS[tier] then
        cru.log("warn", "Discord plugin: unknown access tier '" .. tostring(tier) .. "', using read")
        return "read"
    end
    -- `ask` needs one identified person to answer, and a guild channel is not
    -- that: whoever replies first answers for everyone in the room. Degrade
    -- rather than prompt into a crowd.
    if tier == "ask" and guild_id then
        cru.log("info", "Discord plugin: 'ask' is DM-only; using read for guild " .. tostring(guild_id))
        return "read"
    end
    return tier
end

--- Configure the agent for a session with optional overrides from plugin config.
--- Returns true when the session has a usable agent.
function M.configure_agent(session_id, tier)
    local provider = config.get("provider")
    local model = config.get("model")

    if not provider or not model then
        cru.log("warn", "Discord plugin: provider and model must be configured")
        return false
    end

    local agent_config = {
        agent_type = config.get("agent_type", "internal"),
        -- An explicit `tool_policy` replaces the tier wholesale; that is the
        -- escape hatch for an operator who wants `bash` on a personal bot.
        tool_policy = config.get("tool_policy", TIERS[tier or "read"] or READ_TOOLS),
        provider = provider,
        model = model,
        -- The citation sentence is conditional ("when kiln notes were
        -- provided"), not imperative. Precognition only injects on the first
        -- user message of a session (`precognition_gate.rs`) while a channel
        -- session is reused for 15 minutes, so from message two onward there
        -- are no notes in context — an unconditional "cite your sources" would
        -- make the model invent titles rather than admit it had none.
        system_prompt = config.get("system_prompt",
            "You are a knowledgeable assistant in a Discord chat. "
            .. "Conversations here are short — usually one or two exchanges — so make each response count. "
            .. "Be thorough and thoughtful rather than terse; the user may not follow up. "
            .. "Use Discord markdown formatting (bold, code blocks, lists) when it helps clarity. "
            .. "When kiln notes were provided to you, name the note titles you drew on at the end of your reply."),
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
