--- Per-user daily turn cap.
---
--- Split out of `init.lua` for the same reason `routing.lua` was: `init.lua`
--- returns a plugin manifest, so nothing declared inside it can be reached from
--- a test suite, and this is the check that decides whether a stranger's flood
--- gets to spend the operator's API key.
---
--- Turns, not tokens. Usage is only recorded `if let Some(u) = usage`
--- (`agent_manager/messaging/stream.rs`) and `last_usage` is overwritten by
--- every tool-loop iteration, while ACP agents emit usage only conditionally
--- and Discord picks its `agent_type` from config. A token quota therefore
--- reads zero — fails *open* — the moment the configured agent changes. A turn
--- counter cannot read zero.

local M = {}

local config = require("config")

-- user id -> { n = turns charged today, day = day key, replied = bool }
local seen = {}

--- Charge one turn to `user_id`.
---
--- Returns `true` when the turn is within quota, or `false, refusal` when it is
--- not. `refusal` is a message string on the single turn that crosses the cap
--- and `nil` on every turn after it: replying to each message of a flood is
--- itself Discord REST traffic during exactly the scenario the cap exists for.
---
--- `day` is injectable so the rollover is testable; callers pass nothing.
--- No `await` anywhere in here — the caller runs it inline on the gateway
--- receive loop so the check and the increment cannot interleave.
function M.charge(user_id, day)
    day = day or os.date("!%Y-%m-%d")
    local cap = config.get("quota_turns_per_day", 50)

    -- A guild message need not carry an author id; bucketing every such message
    -- together is the fail-closed reading, since a fresh bucket per message
    -- would be no quota at all.
    local key = tostring(user_id)

    local entry = seen[key]
    if not entry or entry.day ~= day then
        entry = { n = 0, day = day, replied = false }
        seen[key] = entry
    end

    if entry.n >= cap then
        if entry.replied then return false end
        entry.replied = true
        return false, string.format(
            "You've used your %d turns for today. Try again tomorrow.", cap)
    end

    entry.n = entry.n + 1
    return true
end

return M
