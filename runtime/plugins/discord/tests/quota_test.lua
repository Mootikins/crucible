--- The per-user daily turn cap.
---
--- Turns, not tokens: usage is only recorded `if let Some(u) = usage`
--- (`agent_manager/messaging/stream.rs`) and `last_usage` is overwritten by
--- each tool-loop iteration, so a token quota reads zero for whole classes of
--- agent and fails *open* on a config flip. A counter cannot read zero.
---
--- The launch criterion is verified by hand as "set the cap to 1, script a
--- flood, count the replies", so the reply count is asserted here literally
--- rather than inferred from the refusal.

local quota = require("quota")

-- `config.get` reads `crucible.config.get("discord." .. key)` inside a pcall,
-- and the test VM has no `crucible.config` at all — so the table is created and
-- then restored, exactly as `routing_test.lua` does.
local function with_config(tbl, fn)
    crucible = crucible or {}
    local had_config = crucible.config
    crucible.config = { get = function(key) return tbl[key] end }
    local ok, err = pcall(fn)
    crucible.config = had_config
    if not ok then error(err) end
end

local CAP_1 = { ["discord.quota_turns_per_day"] = 1 }

describe("charge", function()
    it("allows turns up to the cap", function()
        with_config(CAP_1, function()
            assert.equals(true, quota.charge("under", "2026-01-01"))
        end)
    end)

    it("refuses every turn past the cap and replies exactly once", function()
        with_config(CAP_1, function()
            local allowed, replies = {}, 0
            for i = 1, 3 do
                local ok, refusal = quota.charge("flooder", "2026-01-01")
                allowed[i] = ok
                if refusal then replies = replies + 1 end
            end

            assert.deep_equal({ true, false, false }, allowed)
            assert.equals(1, replies)
        end)
    end)

    it("names the cap in the one reply it sends", function()
        with_config(CAP_1, function()
            quota.charge("named", "2026-01-01")
            local _, refusal = quota.charge("named", "2026-01-01")
            assert.is_not_nil(refusal:find("1", 1, true))
        end)
    end)

    it("counts each user separately", function()
        with_config(CAP_1, function()
            quota.charge("noisy", "2026-01-01")
            assert.equals(false, quota.charge("noisy", "2026-01-01"))
            assert.equals(true, quota.charge("quiet", "2026-01-01"))
        end)
    end)

    it("resets on the next day, reply included", function()
        with_config(CAP_1, function()
            quota.charge("daily", "2026-01-01")
            assert.equals(false, quota.charge("daily", "2026-01-01"))

            assert.equals(true, quota.charge("daily", "2026-01-02"))
            local ok, refusal = quota.charge("daily", "2026-01-02")
            assert.equals(false, ok)
            assert.is_not_nil(refusal)
        end)
    end)

    -- A guild message need not carry an author id. Bucketing the anonymous
    -- case together is the fail-closed choice: a per-message fresh bucket would
    -- be no quota at all.
    it("shares one bucket for messages with no author id", function()
        with_config(CAP_1, function()
            assert.equals(true, quota.charge(nil, "2026-01-01"))
            assert.equals(false, quota.charge(nil, "2026-01-01"))
        end)
    end)

    it("falls back to the plugin.yaml default when the key is unset", function()
        with_config({}, function()
            for _ = 1, 50 do
                assert.equals(true, quota.charge("defaulted", "2026-01-01"))
            end
            assert.equals(false, quota.charge("defaulted", "2026-01-01"))
        end)
    end)
end)
