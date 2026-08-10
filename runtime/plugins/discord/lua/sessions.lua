--- Discord sender-to-session mapping
--- Manages Crucible agent sessions per Discord channel and speaker.

local config = require("config")

local M = {}

-- "<channel_id>\0<author_id>" -> entry, where an entry is
-- { session_id, last_active, guild_id, tier, key }.
--
-- Keyed per *sender*, not per channel. A DM channel holds one account, so DM
-- behaviour is unchanged; a guild channel now gives each speaker their own
-- session. That is what makes per-sender tiers, quota attribution and one
-- permission prompt per person possible — and it stops one person's long
-- tool-heavy turn from bloating everyone else's context.
local sender_sessions = {}

-- bot_message_id -> entry, for messages the bot has sent. An incoming reply
-- pointing at one continues that session rather than the sender's own.
--
-- In memory and unpersisted, deliberately: a reply to a message the bot sent
-- before a restart simply falls through to a new session, which costs a lost
-- thread rather than a wrong one.
local bot_messages = {}

-- triggering_message_id -> { entry, at }. The bot's own messages echo back over
-- the gateway carrying the id of the message they answered, which is how a bot
-- message is attributed to a session exactly, rather than guessed at from
-- whichever turn happened to be in flight in that channel.
local dispatched = {}

-- Session inactivity timeouts (seconds)
local DM_SESSION_TTL      = 86400   -- 24 hours for DMs
local CHANNEL_SESSION_TTL = 900     -- 15 minutes for channel @mentions
local STALE_TTL           = 7200    -- 2 hours before ending idle sessions
-- Long enough to outlive a turn (the responder's own timeout is 120s) and
-- short enough that a turn which never sent anything does not linger.
local DISPATCH_TTL        = 300

--- Get the session TTL based on context (DM vs channel).
local function session_ttl(guild_id)
    if not guild_id then
        return DM_SESSION_TTL
    end
    return CHANNEL_SESSION_TTL
end

local function sender_key(channel_id, author_id)
    return tostring(channel_id) .. "\0" .. tostring(author_id or "-")
end

--- Whether this entry is still the live session for its sender. An entry
--- reached through the reply index may have been ended and replaced since.
local function is_live(entry)
    return entry and sender_sessions[entry.key] == entry
end

--- Remember which session a message routed to, so the bot's answer to it can
--- be indexed when the gateway echoes it back.
local function note_dispatch(message_id, entry)
    if message_id then
        dispatched[tostring(message_id)] = { entry = entry, at = os.time() }
    end
end

--- Index a message the bot sent against the session that produced it.
---
--- `triggering_message_id` is the id the bot's reply references — the message
--- that started the turn. Anything we did not dispatch is ignored: an
--- unattributed bot message must index nothing rather than the wrong session.
function M.note_bot_message(bot_message_id, triggering_message_id)
    if not bot_message_id or not triggering_message_id then return end
    local record = dispatched[tostring(triggering_message_id)]
    if not record then return end
    dispatched[tostring(triggering_message_id)] = nil
    if is_live(record.entry) then
        bot_messages[tostring(bot_message_id)] = record.entry
    end
end

--- Get or create a Crucible session for whoever sent this message.
--- Reuses an existing session if it was active within the TTL window.
---
--- `opts.message_id` is the incoming message's id, `opts.reply_to` the id of
--- the message it replies to, if any, and `opts.roles` the sender's guild role
--- ids (`data.member.roles`; absent in a DM).
function M.get_or_create(channel_id, guild_id, author_id, opts)
    opts = opts or {}
    local tier = M.access_tier(guild_id, author_id, opts.roles)
    local key = sender_key(channel_id, author_id)
    local entry = sender_sessions[key]
    local ttl = session_ttl(guild_id)

    -- Reply-chain continuity: a reply to something the bot said continues that
    -- message's session, even when the bot said it to somebody else. The tier
    -- must match, because `tool_policy` is fixed when the agent is configured —
    -- joining across tiers would hand the replier the grant the session was
    -- built with. A mismatch makes them their own session instead of
    -- downgrading this one, which would mean discarding the conversation the
    -- reply was continuing.
    local replied = opts.reply_to and bot_messages[tostring(opts.reply_to)]
    if replied and is_live(replied) and replied.tier == tier
        and os.time() - replied.last_active < session_ttl(replied.guild_id) then
        replied.last_active = os.time()
        note_dispatch(opts.message_id, replied)
        return replied.session_id, nil
    end

    if entry then
        local age = os.time() - entry.last_active
        if age < ttl then
            entry.last_active = os.time()
            note_dispatch(opts.message_id, entry)
            return entry.session_id, nil
        end
        -- Session too old, end it and create fresh. Drop the map entry with
        -- it: the only other place that clears one is the successful-create
        -- overwrite below, which the three error returns skip — so on a
        -- misconfigured daemon every later message from this sender re-ended
        -- the same dead id (swallowed by the pcall) and `active_count` kept
        -- reporting it to `:discord status`.
        pcall(cru.sessions.end_session, entry.session_id)
        sender_sessions[key] = nil
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
    if not M.configure_agent(session.id, tier) then
        pcall(cru.sessions.end_session, session.id)
        return nil, "Discord plugin: could not configure an agent for this session"
    end

    -- The tier is recorded, not just applied: it is what a later reply is
    -- checked against, and `tool_policy` cannot be changed after the fact.
    local created = {
        session_id = session.id,
        last_active = os.time(),
        guild_id = guild_id,
        tier = tier,
        key = key,
    }
    sender_sessions[key] = created
    note_dispatch(opts.message_id, created)

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

--- The tier named by the first of `roles` that appears in the access map.
---
--- Ordered by the member's own role list, because that is the only ordering in
--- the comparison: `access` is a Lua table and `pairs` over it has no defined
--- order, so resolving two granting roles against each other any other way
--- would be a coin toss between them.
---
--- Requires a guild. A role id means nothing outside the guild that issued it,
--- and a DM event carries no `member` to read one from in the first place.
local function role_tier(access, guild_id, roles)
    if not guild_id or type(roles) ~= "table" then return nil end
    for _, role_id in ipairs(roles) do
        local tier = access["role:" .. tostring(role_id)]
        if tier then return tier end
    end
    return nil
end

--- The access tier for whoever triggered this turn.
---
--- Keyed on the identities Discord gives us: the account that sent the message,
--- the roles it holds in the guild (`data.member.roles`, absent in a DM), and
--- the guild itself. Precedence is `user:` > `role:` > `guild:` > `default`,
--- first match wins — the sender's own grant beats a role's, and a role's beats
--- the room's.
---
--- A role sits above `guild:` because holding one is something an administrator
--- did on purpose, to that person; being in the guild is not. That is also why
--- a role may grant `write` — it names a principal as surely as an account
--- does, with the operational advantage that a new moderator has access the
--- moment the role is granted, with no config edit and no restart.
---
--- That ordering is only safe because sessions are keyed per sender: a `user:`
--- grant no longer leaks to whoever else is in the channel, because nobody else
--- is in that session. A `guild:` key is a floor for the unnamed rather than a
--- ceiling on the named. All three identities are stable for the life of a
--- sender's session, which matters — the agent config is fixed when the session
--- is created and every later message from them reuses it.
---
---     [plugins.discord.access]
---     "user:1234" = "write"     # this account, wherever it speaks
---     "role:9012" = "write"     # anyone holding that server role
---     "guild:5678" = "read"     # everyone else in that server may look only
---     default = "read"
function M.access_tier(guild_id, author_id, roles)
    local access = config.get("access", {})
    if type(access) ~= "table" then return "read" end

    local user_key = author_id and ("user:" .. tostring(author_id))
    local guild_key = guild_id and ("guild:" .. tostring(guild_id))
    local tier = (user_key and access[user_key])
        or role_tier(access, guild_id, roles)
        or (guild_key and access[guild_key])
        or access.default
        or "read"
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

--- End and remove stale sessions (inactive > STALE_TTL), and drop the routing
--- index entries that outlived them.
function M.cleanup_stale()
    local now = os.time()
    local to_remove = {}

    for key, entry in pairs(sender_sessions) do
        if now - entry.last_active > STALE_TTL then
            pcall(cru.sessions.end_session, entry.session_id)
            table.insert(to_remove, key)
        end
    end

    for _, key in ipairs(to_remove) do
        sender_sessions[key] = nil
    end

    -- Both indexes hold references to entries rather than ids, so a swept
    -- session is recognisable here and a reply that reaches one falls through
    -- to a fresh session instead of resurrecting a dead id.
    for message_id, entry in pairs(bot_messages) do
        if not is_live(entry) then bot_messages[message_id] = nil end
    end

    for message_id, record in pairs(dispatched) do
        if now - record.at > DISPATCH_TTL or not is_live(record.entry) then
            dispatched[message_id] = nil
        end
    end
end

--- Get current session count.
function M.active_count()
    local count = 0
    for _ in pairs(sender_sessions) do
        count = count + 1
    end
    return count
end

return M
