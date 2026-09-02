--!strict
-- The runner VM has no `cru.schedule` (the daemon registers it), so the
-- stub records each timer the plugin asks for.
local scheduled: { any } = {}
;(cru :: any).schedule = function(spec: any, fn: any): number
  scheduled[#scheduled + 1] = { spec = spec, fn = fn }
  return #scheduled
end

--- The reflection plugin is not loaded in the runner VM. The plugin reads
--- its table from `package.loaded`, so a stub there stands in for it. The
--- stub marks a session as aux when the row carries `aux = true`, and it
--- counts user turns the way `build_outcome` does.
local function count_user_turns(messages: { any }): number
  local n = 0
  for _, m in ipairs(messages) do
    if m.role == "user" then n += 1 end
  end
  return n
end

package.loaded["reflection"] = {
  reflection_marker = "<!-- test:reflection-marker -->",
  is_reflection_session = function(s: any): boolean
    return type(s) == "table" and s.aux == true
  end,
  build_outcome = function(messages: { any }, hunks: any): any
    return { user_turns = count_user_turns(messages), tool_errors = 0, hunks_rejected = 0 }
  end,
  build_transcript = function(messages: { any }, opts: any): string
    return "transcript"
  end,
  read_rejected = function(kiln: string): { any } return {} end,
  rejected_titles = function(rejected: { any }, limit: number): { string } return {} end,
  parse_proposals = function(text: string): { any }? return {} end,
  stage_proposals = function(kiln: string, source: string, proposals: { any }, opts: any): { string }
    return {}
  end,
}

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
        { id = "aux", state = "ended", started_at = "2026-09-01T12:30:00Z", aux = true },
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

  describe("run", function()
    local config = require("config")

    --- Two ended sessions: `short` has one user turn, the way a reviewer's
    --- aux session does; `long` has three. The daemon's list rows carry no
    --- `system_prompt`, so the turn count is the guard that holds there.
    local messages_by_id: { [string]: { any } } = {
      short = {
        { role = "user", content = "review this" },
        { role = "assistant", content = "[]" },
      },
      long = {
        { role = "user", content = "one" },
        { role = "assistant", content = "a" },
        { role = "user", content = "two" },
        { role = "assistant", content = "b" },
        { role = "user", content = "three" },
        { role = "assistant", content = "c" },
      },
    }

    before_each(function()
      test_mocks.setup({
        sessions = {
          response_parts = { { type = "text", content = "[]" } },
        },
      })
      cru.session.list = function()
        return {
          { id = "short", state = "ended", started_at = "2026-09-01T10:00:00Z" },
          { id = "long", state = "ended", started_at = "2026-09-01T11:00:00Z" },
        }, nil
      end
      cru.session.messages = function(id: string, opts: any)
        return messages_by_id[id]
      end
      config.reset()
      plugin.setup({ kiln = "notes", model = "test-model" })
    end)

    after_each(function()
      test_mocks.reset()
      config.reset()
    end)

    it("drops a session with fewer user turns than min_turns", function()
      plugin.run(nil)

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      local prompt: any = sent[1][2]
      expect.truthy(prompt:find("### session long", 1, true))
      expect.is_nil(prompt:find("### session short", 1, true))
    end)

    it("keeps a one-turn session when min_turns allows it", function()
      plugin.setup({ min_turns = 1 })
      plugin.run(nil)

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      local prompt: any = sent[1][2]
      expect.truthy(prompt:find("### session short", 1, true))
      expect.truthy(prompt:find("### session long", 1, true))
    end)
  end)
end)
