--!strict
-- The runner VM has no `cru.schedule` (the daemon registers it), so the
-- stub records each timer the plugin asks for.
local scheduled: { any } = {}
;(cru :: any).schedule = function(spec: any, fn: any): number
  scheduled[#scheduled + 1] = { spec = spec, fn = fn }
  return #scheduled
end

describe("consolidation", function()
  local plugin = require("consolidation")

  describe("load", function()
    it("registers one timer with the configured interval", function()
      expect.equal(1, #scheduled)
      expect.equal(21600, scheduled[1].spec.every)
    end)
  end)

  describe("stratify", function()
    it("puts sessions with problems first and caps each group", function()
      local sessions = {
        { id = "clean1", outcome = { tool_errors = 0, hunks_rejected = 0 } },
        { id = "bad1", outcome = { tool_errors = 2, hunks_rejected = 0 } },
        { id = "clean2", outcome = { tool_errors = 0, hunks_rejected = 0 } },
        { id = "bad2", outcome = { tool_errors = 0, hunks_rejected = 1 } },
        { id = "clean3", outcome = { tool_errors = 0, hunks_rejected = 0 } },
        { id = "clean4", outcome = { tool_errors = 0, hunks_rejected = 0 } },
      }
      local picked = plugin.stratify(sessions, { max_problem = 5, max_clean = 3 })
      expect.equal(5, #picked)
      expect.equal("bad1", picked[1].id)
      expect.equal("bad2", picked[2].id)
      expect.equal("clean1", picked[3].id)
    end)

    it("caps the problem group on its own", function()
      local sessions = {
        { id = "bad1", outcome = { tool_errors = 1, hunks_rejected = 0 } },
        { id = "bad2", outcome = { tool_errors = 1, hunks_rejected = 0 } },
        { id = "bad3", outcome = { tool_errors = 1, hunks_rejected = 0 } },
      }
      local picked = plugin.stratify(sessions, { max_problem = 2, max_clean = 3 })
      expect.equal(2, #picked)
      expect.equal("bad2", picked[2].id)
    end)
  end)

  describe("since_cursor", function()
    it("keeps only sessions that ended after the cursor and are not aux sessions", function()
      local list = {
        { id = "a", state = "ended", started_at = "2026-09-01T10:00:00Z" },
        { id = "b", state = "ended", started_at = "2026-09-01T12:00:00Z" },
        { id = "c", state = "active", started_at = "2026-09-01T13:00:00Z" },
      }
      local got = plugin.since_cursor(list, "2026-09-01T11:00:00Z")
      expect.equal(1, #got)
      expect.equal("b", got[1].id)
    end)

    it("keeps every ended session when there is no cursor, oldest first", function()
      local list = {
        { id = "late", state = "ended", started_at = "2026-09-01T12:00:00Z" },
        { id = "early", state = "ended", started_at = "2026-09-01T10:00:00Z" },
      }
      local got = plugin.since_cursor(list, nil)
      expect.equal(2, #got)
      expect.equal("early", got[1].id)
      expect.equal("late", got[2].id)
    end)
  end)

  describe("build_prompt", function()
    it("asks for pattern notes with a problem, cause and fix description", function()
      local p = plugin.build_prompt({ { id = "s1", transcript = "## user\nx" } }, { "Rejected title" })
      expect.truthy(p:find("### session s1", 1, true))
      expect.truthy(p:find("problem; root cause; fix", 1, true))
      expect.truthy(p:find("Rejected title", 1, true))
    end)

    it("omits the rejection block when nothing was rejected", function()
      local p = plugin.build_prompt({ { id = "s1", transcript = "## user\nx" } }, {})
      expect.is_nil(p:find("rejected", 1, true))
    end)
  end)
end)
