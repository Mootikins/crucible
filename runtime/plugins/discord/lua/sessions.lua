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

-- Where the DM half of the map is kept between daemon restarts, and the shape
-- of what is written there.
local STATE_PLUGIN  = "discord"
local STATE_FILE    = "sessions.json"
local STATE_VERSION = 1

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

--- Where the state file lives, or nil in a runtime without a `paths` module.
---
--- `cru.paths` is registered by the daemon and by nothing else, so a runtime
--- that lacks it — the plugin test VM, most obviously — gets no persistence
--- and full routing. Persistence is an improvement on forgetting, never a
--- prerequisite for answering.
local function state_path()
    local ok, path = pcall(function()
        return cru.paths.join(cru.paths.state(STATE_PLUGIN), STATE_FILE)
    end)
    if ok and type(path) == "string" then return path end
    return nil
end

--- The DM entries of `map`, as the blob that goes to disk.
---
--- DMs only, and this is the whole of D8: a channel session lives 900 seconds,
--- so persisting one buys at most fifteen minutes of continuity in exchange
--- for a write on every channel message. A DM lives 24 hours and is the
--- conversation somebody expects to still be there tomorrow.
---
--- The reply-chain index is not written either. It keys on Discord message ids
--- whose sessions this file may not carry, and a reply to something said
--- before a restart falling through to a new session costs a lost thread
--- rather than a wrong one.
local function encode_dms(map)
    local entries = {}
    for key, entry in pairs(map) do
        if not entry.guild_id and entry.session_id then
            table.insert(entries, {
                key = key,
                session_id = entry.session_id,
                last_active = entry.last_active,
                tier = entry.tier,
            })
        end
    end
    return cru.json.encode({ version = STATE_VERSION, entries = entries })
end

--- Write the DM sessions of `map` to `path`. Returns whether it landed.
function M.save_to(path, map)
    if not path then return false end
    local ok, err = pcall(cru.fs.write, path, encode_dms(map))
    if not ok then
        cru.log("warn", "Discord plugin: could not persist sessions: " .. tostring(err))
    end
    return ok
end

--- Read the DM sessions back from `path`, keyed as the live map keys them.
---
--- Anything unreadable is treated as no state at all: a corrupt blob costs the
--- conversations it held, not the plugin's ability to answer the next message.
function M.load_from(path)
    if not path then return {} end
    local exists_ok, exists = pcall(cru.fs.exists, path)
    if not exists_ok or not exists then return {} end

    local read_ok, raw = pcall(cru.fs.read, path)
    if not read_ok then return {} end

    local decode_ok, blob = pcall(cru.json.decode, raw)
    if not decode_ok or type(blob) ~= "table" or type(blob.entries) ~= "table" then
        cru.log("warn", "Discord plugin: session state at " .. path .. " is unreadable; starting empty")
        return {}
    end

    local now = os.time()
    local restored = {}
    for _, record in ipairs(blob.entries) do
        local last_active = tonumber(record.last_active) or 0
        -- An entry already past the DM TTL would be ended and replaced by the
        -- very next message. Reviving it to do that costs a daemon round trip
        -- and leaves `:discord status` counting sessions nobody can reach.
        if type(record.key) == "string" and type(record.session_id) == "string"
            and now - last_active < DM_SESSION_TTL then
            restored[record.key] = {
                session_id = record.session_id,
                last_active = last_active,
                guild_id = nil,
                tier = record.tier or "read",
                key = record.key,
            }
        end
    end
    return restored
end

--- Merge the persisted DM sessions into the live map.
---
--- A key that is already live wins: the file records what was, the map holds
--- what is, and a restore must never displace a session mid-conversation.
function M.restore()
    for key, entry in pairs(M.load_from(state_path())) do
        if sender_sessions[key] == nil then
            sender_sessions[key] = entry
        end
    end
end

local loaded = false

--- Restore once, on first use.
---
--- Lazily rather than from `init.lua`, because `init.lua` returns a manifest
--- and the map lives here: the file that owns the state owns reloading it, and
--- no caller has to remember to ask.
local function ensure_loaded()
    if loaded then return end
    loaded = true
    M.restore()
end

--- Persist, when the entry that changed was a DM. Guild sessions are not in
--- the file, so a guild mutation would rewrite it byte-identical.
local function persist(guild_id)
    if guild_id then return end
    M.save_to(state_path(), sender_sessions)
end

--- Get or create a Crucible session for whoever sent this message.
--- Reuses an existing session if it was active within the TTL window.
---
--- `opts.message_id` is the incoming message's id, `opts.reply_to` the id of
--- the message it replies to, if any, and `opts.roles` the sender's guild role
--- ids (`data.member.roles`; absent in a DM).
function M.get_or_create(channel_id, guild_id, author_id, opts)
    opts = opts or {}
    ensure_loaded()
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
    --
    -- Whose session a reply may join depends on the deployment shape. A
    -- personal bot has one correspondent, so "somebody else" does not exist and
    -- the chain is free. A server has several, and joining across senders hands
    -- the replier the other person's context -- the tier check above does not
    -- catch it, because at the default everybody is `read`. Server mode
    -- therefore requires the same sender unless `share_reply_chains` asks for
    -- the collaborative behaviour back.
    local may_join_another_sender = config.mode() == "personal"
        or config.get("share_reply_chains", false)

    local replied = opts.reply_to and bot_messages[tostring(opts.reply_to)]
    if replied and is_live(replied) and replied.tier == tier
        and (may_join_another_sender or replied.key == key)
        and os.time() - replied.last_active < session_ttl(replied.guild_id) then
        replied.last_active = os.time()
        note_dispatch(opts.message_id, replied)
        persist(replied.guild_id)
        return replied.session_id, nil
    end

    if entry then
        local age = os.time() - entry.last_active
        if age < ttl then
            entry.last_active = os.time()
            note_dispatch(opts.message_id, entry)
            -- The bump is persisted, not just the creation: `load_from` drops
            -- what is already past the DM TTL, so a file that never learns a
            -- 24-hour conversation is still being had would discard it the
            -- first time the daemon restarted a day in.
            persist(entry.guild_id)
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
        persist(guild_id)
    end

    -- The kiln is required, not defaulted: a session with no kiln has no note
    -- tools at all, which stages reflection proposals under the daemon's data
    -- root where `cru proposals list` never looks. Refuse before creating
    -- anything.
    --
    -- It is the NAME of a `[kilns]` entry, not a directory — `cru.sessions.create`
    -- takes names. A path is not a name and resolves to nothing, so a config
    -- carried over from the path era produces a kiln-less session; the daemon
    -- warns and this plugin's writes land nowhere useful.
    local kiln = config.get("kiln")
    if not kiln then
        return nil, "Discord plugin: no kiln configured — set [plugins.discord] kiln (a [kilns] entry name) in your Crucible config"
    end

    -- One flat set with no primary member, so `kiln` is simply the first
    -- entry: position is what the daemon reads to pick a write target, and
    -- there is no separate field that says "write here".
    local create_opts = { type = "chat", kilns = { kiln } }

    -- Additional read-only kilns, if any are configured.
    for _, extra in ipairs(config.get("kilns") or {}) do
        if extra ~= kiln then
            create_opts.kilns[#create_opts.kilns + 1] = extra
        end
    end

    -- An agent card is resolved by the daemon at create, against this session's
    -- kiln — the plugin cannot look one up itself, and the post-create
    -- `configure_agent` below would overwrite whatever the card supplied. So on
    -- this path the whole agent, tier included, is settled in one call.
    local agent_card = config.get("agent_card")
    if agent_card then
        if config.get("agent_type", "internal") ~= "internal" then
            return nil, "Discord plugin: agent_card configures the internal agent; it cannot be combined with agent_type = \"acp\""
        end
        if config.get("agent_name") then
            return nil, "Discord plugin: set agent_card or agent_name, not both — agent_name names an ACP profile"
        end
        local provider = config.get("provider")
        local model = config.get("model")
        if not provider or not model then
            return nil, "Discord plugin: provider and model must be configured"
        end
        create_opts.agent_card = agent_card
        create_opts.provider = provider
        create_opts.provider_key = config.get("provider_key")
        create_opts.model = model
        -- Last word over the card's own `tools:` block: the tier is this
        -- sender's grant, the card is a file the operator wrote once.
        create_opts.tool_policy = M.tool_policy_for(tier)
    end

    local session, err = cru.sessions.create(create_opts)
    if not session then
        return nil, "Failed to create session: " .. tostring(err)
    end

    -- A session whose agent could not be configured answers every message with
    -- "NoAgentConfigured" for the full TTL if it is cached, so end it and
    -- report the failure instead.
    if not agent_card and not M.configure_agent(session.id, tier) then
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
    persist(guild_id)

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
--- differently. Reads are bounded by the session's `RootSet` allowlist — its
--- attached kilns, its workspace and its own session dir, default-deny outside
--- them, with transcript directories and the trees the daemon executes Lua from
--- carved out — so "read" means *within the kilns you configured*, not the
--- filesystem.
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
    grep_notes = "allow",
}

--- The read set plus the tools that change the kiln. Still no `bash`: its blast
--- radius is not bounded by the `RootSet` allowlist, so it stays a deliberate opt-in
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
--- Who answers is `approvers`: configured, the prompt goes to the first
--- approver's DM and only they resolve it, so the request may come from
--- anywhere. Unconfigured, the requester answers their own prompt, which
--- `M.access_tier` only allows when a `user:` or `role:` key named them.
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

--- The accounts that answer permission prompts, in order, from `approvers`.
---
--- Empty means the requester answers their own — which is only offered to
--- someone a `user:` or `role:` key named. Put your own id here and the
--- personal case is the delegated one with both parties being you.
function M.approvers()
    local list = config.get("approvers", {})
    if type(list) ~= "table" then return {} end
    return list
end

--- Which kind of key granted a tier, alongside the tier itself. The source is
--- what decides whether an `ask` grant has a principal behind it: `user:` and
--- `role:` name accounts, `guild:` and `default` name a room.
local function granted_tier(access, guild_id, author_id, roles)
    local user_key = author_id and ("user:" .. tostring(author_id))
    if user_key and access[user_key] then return access[user_key], "user" end

    local from_role = role_tier(access, guild_id, roles)
    if from_role then return from_role, "role" end

    local guild_key = guild_id and ("guild:" .. tostring(guild_id))
    if guild_key and access[guild_key] then return access[guild_key], "guild" end

    return access.default or "read", "default"
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
---     [plugins.discord]
---     approvers = ["1234"]      # who answers an `ask` prompt, in their DM
---
---     [plugins.discord.access]
---     "user:1234" = "write"     # this account, wherever it speaks
---     "role:9012" = "write"     # anyone holding that server role
---     "guild:5678" = "read"     # everyone else in that server may look only
---     default = "read"
function M.access_tier(guild_id, author_id, roles)
    local access = config.get("access", {})
    if type(access) ~= "table" then return "read" end

    local tier, source = granted_tier(access, guild_id, author_id, roles)
    if not TIERS[tier] then
        cru.log("warn", "Discord plugin: unknown access tier '" .. tostring(tier) .. "', using read")
        return "read"
    end
    -- `ask` needs one identified account to answer. With `approvers` set that
    -- is the approver, prompted in their own DM, and the request may come from
    -- anywhere. Without one the requester answers, so the grant must have named
    -- them: `guild:` and `default` describe the room, and prompting a room is
    -- how the first person to type answers for everyone in it.
    -- A server has no account that is both parties. Self-approval is the point
    -- of `ask` on a personal bot and the hole approvals exist to close on a
    -- shared one, so server mode needs a named approver whatever the grant's
    -- source; personal mode keeps the narrower rule, where only a key naming a
    -- ROOM (`guild:`, `default`) cannot answer for itself.
    local needs_approver = config.mode() == "server"
        or source == "guild" or source == "default"
    if tier == "ask" and #M.approvers() == 0 and needs_approver then
        cru.log("info",
            "Discord plugin: 'ask' from a " .. source .. " key needs an approvers list; using read")
        return "read"
    end
    return tier
end

--- The tool policy a session at `tier` runs under.
---
--- One function because the tier reaches the agent by two routes now — in
--- `create_opts` on the agent-card path, in `configure_agent` otherwise — and a
--- second copy is how one of them ends up a tier behind.
---
--- An explicit `tool_policy` replaces the tier wholesale; that is the escape
--- hatch for an operator who wants `bash` on a personal bot.
function M.tool_policy_for(tier)
    return config.get("tool_policy", TIERS[tier or "read"] or READ_TOOLS)
end

--- Configure the agent for a session with optional overrides from plugin config.
--- Returns true when the session has a usable agent.
---
--- Not called on the agent-card path: `configure_agent` writes the *whole*
--- agent, so running it after a card was resolved at create would replace the
--- card's prompt and model with the ones below. See `M.get_or_create`.
function M.configure_agent(session_id, tier)
    local provider = config.get("provider")
    local model = config.get("model")

    if not provider or not model then
        cru.log("warn", "Discord plugin: provider and model must be configured")
        return false
    end

    local agent_config = {
        agent_type = config.get("agent_type", "internal"),
        tool_policy = M.tool_policy_for(tier),
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

    -- `agent_name` names an ACP *profile* and nothing else. On an internal
    -- agent it resolves no card and never did — it only ever set a field
    -- nothing reads — so refuse it rather than let it look configured. A card
    -- has to be named at create, where the daemon can resolve it against the
    -- session's kiln; `configure_agent` has no kiln to resolve against.
    local agent_name = config.get("agent_name")
    if agent_name then
        if agent_config.agent_type ~= "acp" then
            cru.log("warn",
                "Discord plugin: [plugins.discord] agent_name names an ACP profile and needs "
                .. "agent_type = \"acp\". For an internal agent persona set agent_card instead — "
                .. "it is resolved at create (`cru.sessions.create{ agent_card = ... }`).")
            return false
        end
        agent_config.agent_name = agent_name
    end

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
    ensure_loaded()
    local now = os.time()
    local to_remove = {}
    local swept_a_dm = false

    for key, entry in pairs(sender_sessions) do
        if now - entry.last_active > STALE_TTL then
            pcall(cru.sessions.end_session, entry.session_id)
            table.insert(to_remove, key)
            if not entry.guild_id then swept_a_dm = true end
        end
    end

    for _, key in ipairs(to_remove) do
        sender_sessions[key] = nil
    end

    -- A session the sweep ended must not come back on the next restart.
    if swept_a_dm then persist(nil) end

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
    ensure_loaded()
    local count = 0
    for _ in pairs(sender_sessions) do
        count = count + 1
    end
    return count
end

return M
