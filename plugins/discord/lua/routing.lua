--- Who the bot answers.
---
--- Split out of `init.lua` because this is the decision that spends the
--- operator's API key on a stranger's message, and `init.lua` returns a plugin
--- manifest rather than a module — so nothing in it can be reached from a
--- test. A pure function taking `bot_user_id` as an argument keeps it that way.

local M = {}

local config = require("config")

--- Whether `id` appears in the `discord.<key>` allowlist.
---
--- Empty (the default) means *nobody*, not *everybody*. Discord ids arrive
--- from the gateway as strings but are routinely written unquoted in TOML, so
--- both sides are compared as strings.
local function allowed(key, id)
    local list = config.get(key, {})
    if type(list) ~= "table" or not id then return false end
    for _, entry in ipairs(list) do
        if tostring(entry) == tostring(id) then return true end
    end
    return false
end

--- Check whether a gateway message should trigger a bot response.
---
--- `bot_user_id` is the id captured from READY, passed in rather than read
--- from module state so this stays a pure function.
function M.should_respond(data, bot_user_id)
    -- Never respond to bots
    if data.author and data.author.bot then return false end

    local content = data.content or ""
    local respond_to = config.get("respond_to", "mentions")

    -- Allowlists first, above every routing rule including the DM branch.
    -- The DM branch used to return `true` unconditionally and sat *above* the
    -- `respond_to` check, so no configuration value could close it and anyone
    -- who could DM the bot could spend the operator's key.
    if data.guild_id then
        if not allowed("allowed_guilds", data.guild_id) then return false end
    elseif not allowed("allowed_users", data.author and data.author.id) then
        return false
    end

    -- DMs (guild_id is nil for DMs)
    if not data.guild_id then return true end

    -- Check @mention
    if bot_user_id and respond_to ~= "prefix" then
        if content:find("<@" .. bot_user_id .. ">") or content:find("<@!" .. bot_user_id .. ">") then
            return true
        end
    end

    -- Check prefix
    local prefix = config.get("command_prefix", "")
    if prefix ~= "" and (respond_to == "prefix" or respond_to == "both") then
        if content:sub(1, #prefix) == prefix then
            return true
        end
    end

    -- Respond to all messages in channel
    if respond_to == "all" then return true end

    return false
end

return M
