--!strict
--- Who the bot will answer.
---
--- Every path that spends the operator's API key on a stranger's message goes
--- through `should_respond`, and the DM branch used to return `true`
--- unconditionally *above* the `respond_to` check — so no configuration value
--- closed it. These tests exist so that cannot come back silently.

-- The runner VM has no cru.plugin (the daemon registers it); tests stub into it.
cru.plugin = cru.plugin or mock({})
local routing = require("routing")

-- `config.get` reads `cru.plugin.config.get("discord." .. key)` inside a pcall,
-- so the suite stubs that lookup rather than the plugin's own accessor — the
-- key-prefixing and the default-on-missing behaviour stay under test. The test
-- VM has no `cru.plugin.config`, hence the table is created and then restored.
local function with_config(tbl: { [string]: any }, fn: () -> ())
    local had_config = cru.plugin.config
    cru.plugin.config = mock({ get = function(key) return tbl[key] end })
    -- The inner function returns nil so `pcall` has a second slot for the
    -- error to bind to: `fn` answers with nothing.
    local ok, err = pcall(function()
        fn()
        return nil
    end)
    cru.plugin.config = had_config
    if not ok then error(err) end
end

local function dm_from(user_id: string): { [string]: any }
    return { content = "hello", author = { id = user_id } }
end

local function guild_message(guild_id: string, content: string?): { [string]: any }
    return { content = content or "hello", guild_id = guild_id, author = { id = "u1" } }
end

describe("should_respond", function()
    it("ignores a DM from an unlisted user", function()
        with_config({}, function()
            expect.equals(false, routing.should_respond(dm_from("stranger")))
        end)
    end)

    it("answers a DM from a listed user", function()
        with_config({ ["discord.allowed_users"] = { "friend" } }, function()
            expect.equals(true, routing.should_respond(dm_from("friend")))
        end)
    end)

    it("still ignores a DM from a user who is not on a non-empty list", function()
        with_config({ ["discord.allowed_users"] = { "friend" } }, function()
            expect.equals(false, routing.should_respond(dm_from("stranger")))
        end)
    end)

    it("ignores a guild message when the guild is unlisted", function()
        with_config({ ["discord.respond_to"] = "all" }, function()
            expect.equals(false, routing.should_respond(guild_message("g1")))
        end)
    end)

    it("answers a guild message once the guild is listed", function()
        with_config({
            ["discord.allowed_guilds"] = { "g1" },
            ["discord.respond_to"] = "all",
        }, function()
            expect.equals(true, routing.should_respond(guild_message("g1")))
        end)
    end)

    -- Ids arrive from the gateway as strings but are written unquoted in TOML
    -- often enough that a numeric allowlist entry must still match.
    it("matches a numeric allowlist entry against a string id", function()
        with_config({ ["discord.allowed_users"] = { 12345 } }, function()
            expect.equals(true, routing.should_respond(dm_from("12345")))
        end)
    end)

    it("never answers another bot, listed or not", function()
        with_config({ ["discord.allowed_users"] = { "botty" } }, function()
            expect.equals(false, routing.should_respond({
                content = "hello",
                author = { id = "botty", bot = true },
            }))
        end)
    end)
end)

-- The mention and prefix matchers had no coverage at all: every case above
-- either takes the DM branch or sets `respond_to = "all"`, and none passed a
-- `bot_user_id` — production calls `should_respond(data, bot_user_id)` with
-- two arguments. Deleting the whole matcher left the suite green, and
-- `mentions` is the documented default, so this was the one guild path an
-- unconfigured operator actually gets.
describe("should_respond guild matching", function()
    local BOT = "botid"

    local function guild_msg(content: string): { [string]: any }
        return { content = content, guild_id = "g1", author = { id = "u1" } }
    end

    local ALLOWED = { ["discord.allowed_guilds"] = { "g1" } }

    it("answers a plain @mention under the default respond_to", function()
        with_config(ALLOWED, function()
            expect.equals(true, routing.should_respond(guild_msg("hey <@botid> hi"), BOT))
        end)
    end)

    it("answers the nickname form of a mention", function()
        with_config(ALLOWED, function()
            expect.equals(true, routing.should_respond(guild_msg("<@!botid> hi"), BOT))
        end)
    end)

    it("ignores a guild message that mentions nobody", function()
        with_config(ALLOWED, function()
            expect.equals(false, routing.should_respond(guild_msg("just chatting"), BOT))
        end)
    end)

    it("ignores a mention of somebody else", function()
        with_config(ALLOWED, function()
            expect.equals(false, routing.should_respond(guild_msg("<@someoneelse> hi"), BOT))
        end)
    end)

    it("answers a prefixed message when respond_to is prefix", function()
        local cfg = { ["discord.allowed_guilds"] = { "g1" },
                      ["discord.respond_to"] = "prefix",
                      ["discord.command_prefix"] = "!" }
        with_config(cfg, function()
            expect.equals(true, routing.should_respond(guild_msg("!ask something"), BOT))
            expect.equals(false, routing.should_respond(guild_msg("no prefix here"), BOT))
        end)
    end)

    -- `respond_to = "prefix"` means the prefix *instead of* mentions, and the
    -- matcher skips the mention branch entirely for it.
    it("ignores a bare mention when respond_to is prefix", function()
        local cfg = { ["discord.allowed_guilds"] = { "g1" },
                      ["discord.respond_to"] = "prefix",
                      ["discord.command_prefix"] = "!" }
        with_config(cfg, function()
            expect.equals(false, routing.should_respond(guild_msg("<@botid> hi"), BOT))
        end)
    end)
end)
