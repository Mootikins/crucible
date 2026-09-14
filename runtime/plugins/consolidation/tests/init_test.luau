--!strict
-- The stub replaces `cru.schedule` and records each timer the plugin asks
-- for, so no timer reaches the daemon's scheduler.
local scheduled: { any } = {}
;(cru :: any).schedule = function(spec: any, fn: any): number
  scheduled[#scheduled + 1] = { spec = spec, fn = fn }
  return #scheduled
end

--- The reflection plugin is not loaded in the runner VM. The plugin reads
--- its table from `package.loaded`, so a stub there stands in for it. The
--- stub counts user turns and tool errors the way `build_outcome` does.
local function count_user_turns(messages: { any }): number
  local n = 0
  for _, m in ipairs(messages) do
    if m.role == "user" then n += 1 end
  end
  return n
end

local function count_tool_errors(messages: { any }): number
  local n = 0
  for _, m in ipairs(messages) do
    if m.role == "tool_result" and m.error then n += 1 end
  end
  return n
end

--- Every `review` call the stub saw, and the text the next one answers
--- with. The lifecycle itself is the reflection suite's to test.
local review_calls: { any } = {}
local review_text = "Nothing recurs"

package.loaded["reflection"] = {
  review = function(opts: any): (string?, string?)
    review_calls[#review_calls + 1] = opts
    return review_text, nil
  end,
  load_hunks = function(session_id: string): { any }? return nil end,
  build_outcome = function(messages: { any }, hunks: any): any
    return {
      user_turns = count_user_turns(messages),
      tool_errors = count_tool_errors(messages),
      hunks_rejected = 0,
    }
  end,
  build_transcript = function(messages: { any }, opts: any): string
    return "transcript"
  end,
  --- The reviewer writes its notes through its own session now, so the pass
  --- stages nothing. The real reflection plugin exports no `stage_proposals`
  --- any more; this stub raises, so a pass that calls it fails loudly.
  stage_proposals = function(kiln: string, session: string?, proposals: { any }, opts: any): { string }
    error("consolidation staged a proposal; the pass writes through its own session")
  end,
}

describe("consolidation", function()
  -- The runner activated this plugin before it loaded this file, and seeded
  -- `package.loaded` with the activated instance, whose setup() already ran
  -- against the real `cru.on`. This suite drives setup() itself with the stubs
  -- above and reads their recorders, so it requires a fresh instance.
  package.loaded["consolidation"] = nil
  local plugin = require("consolidation")

  describe("load", function()
    it("registers no timer until setup() runs", function()
      expect.equal(0, #scheduled)
    end)

    it("registers one timer with the configured interval, from setup(), once", function()
      plugin.setup()
      expect.equal(1, #scheduled)
      expect.equal(21600, scheduled[1].spec.every)

      plugin.setup({ interval = 60 })
      expect.equal(1, #scheduled)
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

    it("walks past a session whose group cap is 0", function()
      -- A cap of 0 means the group is never wanted. The walk must not stop
      -- at `clean1`: it skips it, takes `bad1`, and counts both as walked,
      -- so the cursor can move past them.
      local sessions = {
        { id = "clean1", outcome = { tool_errors = 0, hunks_rejected = 0 } },
        { id = "bad1", outcome = { tool_errors = 1, hunks_rejected = 0 } },
      }
      local picked, taken = plugin.stratify(sessions, { max_problem = 5, max_clean = 0 })
      expect.equal(1, #picked)
      expect.equal("bad1", picked[1].id)
      expect.equal(2, taken)
    end)
  end)

  describe("since_cursor", function()
    it("keeps only sessions that ended after the cursor and are not plugin sessions", function()
      -- The daemon's list rows carry `session_type`. A reviewer's own
      -- session is a plugin session, and the sample must leave it out.
      local list = {
        { id = "a", state = "ended", started_at = "2026-09-01T10:00:00Z", session_type = "chat" },
        { id = "b", state = "ended", started_at = "2026-09-01T12:00:00Z", session_type = "chat" },
        { id = "c", state = "active", started_at = "2026-09-01T13:00:00Z", session_type = "chat" },
        { id = "aux", state = "ended", started_at = "2026-09-01T12:30:00Z", session_type = "plugin" },
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
      local p = plugin.build_prompt({ { id = "s1", transcript = "## user\nx" } })
      expect.truthy(p:find("### session s1", 1, true))
      expect.truthy(p:find("problem; root cause; fix", 1, true))
    end)
  end)

  describe("run", function()
    local config = require("config")

    --- Two ended sessions: `short` has one user turn; `long` has three.
    --- `min_turns` is a quality gate on the sample. A plugin's own aux
    --- session is kept out by its `session_type`, not by its turn count.
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
      review_calls = {}
      review_text = "Nothing recurs"
      test_mocks.setup()
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

      expect.equal(1, #review_calls)
      local prompt: any = review_calls[1].prompt
      expect.truthy(prompt:find("### session long", 1, true))
      expect.is_nil(prompt:find("### session short", 1, true))
    end)

    it("keeps a one-turn session when min_turns allows it", function()
      plugin.setup({ min_turns = 1 })
      plugin.run(nil)

      expect.equal(1, #review_calls)
      local prompt: any = review_calls[1].prompt
      expect.truthy(prompt:find("### session short", 1, true))
      expect.truthy(prompt:find("### session long", 1, true))
    end)

    it("asks the reflection plugin for one review in the configured kiln", function()
      plugin.setup({ timeout = 99, provider = "anthropic" })
      plugin.run(nil)

      expect.equal(1, #review_calls)
      local call = review_calls[1]
      expect.equal("notes", call.kiln)
      expect.equal("test-model", call.model)
      expect.equal("anthropic", call.provider)
      expect.equal(plugin.system_prompt, call.system_prompt)
      expect.equal(99, call.timeout)
    end)

    it("stores the newest started_at as the cursor after a pass", function()
      plugin.run(nil)

      expect.equal("2026-09-01T11:00:00Z", cru.storage.get("consolidation", "last_run"))
    end)

    it("moves the cursor only past the sessions the reviewer saw", function()
      -- Three sessions after the cursor, oldest first: two clean, then one
      -- with a tool error. `max_clean = 1` stops the sample at `b`. The
      -- cursor must stop there too, so `b` and `c` are candidates again;
      -- a cursor at the newest candidate would drop `b` for good.
      plugin.setup({ min_turns = 1, max_clean = 1 })
      messages_by_id.a = messages_by_id.short
      messages_by_id.b = messages_by_id.short
      messages_by_id.c = {
        { role = "user", content = "run it" },
        { role = "tool_result", content = "", error = "exit 1" },
      }
      cru.session.list = function()
        return {
          { id = "a", state = "ended", started_at = "2026-09-01T10:00:00Z" },
          { id = "b", state = "ended", started_at = "2026-09-01T11:00:00Z" },
          { id = "c", state = "ended", started_at = "2026-09-01T12:00:00Z" },
        }, nil
      end

      plugin.run(nil)

      expect.equal(1, #review_calls)
      local first: any = review_calls[1].prompt
      expect.truthy(first:find("### session a", 1, true))
      expect.is_nil(first:find("### session b", 1, true))
      expect.equal("2026-09-01T10:00:00Z", cru.storage.get("consolidation", "last_run"))

      plugin.run(nil)

      expect.equal(2, #review_calls)
      local second: any = review_calls[2].prompt
      expect.truthy(second:find("### session b", 1, true))
      expect.truthy(second:find("### session c", 1, true))
      expect.is_nil(second:find("### session a", 1, true))
      expect.equal("2026-09-01T12:00:00Z", cru.storage.get("consolidation", "last_run"))
    end)

    it("skips a session whose transcript did not load and moves the cursor past it", function()
      -- `cru.session.messages` answers `(nil, err)` when the read fails. The
      -- pass reviews the sessions that loaded, and the cursor moves past the
      -- one that did not: a transcript that never loads must not stop every
      -- later pass at the same session.
      plugin.setup({ min_turns = 1 })
      messages_by_id.a = messages_by_id.short
      messages_by_id.b = messages_by_id.short
      messages_by_id.c = messages_by_id.short
      cru.session.list = function()
        return {
          { id = "a", state = "ended", started_at = "2026-09-01T10:00:00Z" },
          { id = "b", state = "ended", started_at = "2026-09-01T11:00:00Z" },
          { id = "c", state = "ended", started_at = "2026-09-01T12:00:00Z" },
        }, nil
      end
      local fail_b = true
      cru.session.messages = function(id: string, opts: any)
        if fail_b and id == "b" then return nil, "rpc: connection reset" end
        return messages_by_id[id], nil
      end

      plugin.run(nil)

      expect.equal(1, #review_calls)
      local first: any = review_calls[1].prompt
      expect.truthy(first:find("### session a", 1, true))
      expect.is_nil(first:find("### session b", 1, true))
      expect.truthy(first:find("### session c", 1, true))
      expect.equal("2026-09-01T12:00:00Z", cru.storage.get("consolidation", "last_run"))

      fail_b = false
      plugin.run(nil)

      -- Nothing newer than the cursor: no second review, and `b` is not
      -- read again.
      expect.equal(1, #review_calls)
    end)

    it("reviews the problem sessions when max_clean is 0", function()
      -- `max_clean = 0` asks for problem sessions only. The oldest candidate
      -- is clean. The pass must walk past it, review `bad`, and move the
      -- cursor: a pass that stops at `clean` reads the same candidates
      -- again forever and reviews nothing.
      plugin.setup({ min_turns = 1, max_clean = 0 })
      messages_by_id.clean = messages_by_id.short
      messages_by_id.bad = {
        { role = "user", content = "run it" },
        { role = "tool_result", content = "", error = "exit 1" },
      }
      cru.session.list = function()
        return {
          { id = "clean", state = "ended", started_at = "2026-09-01T10:00:00Z" },
          { id = "bad", state = "ended", started_at = "2026-09-01T11:00:00Z" },
        }, nil
      end

      plugin.run(nil)

      expect.equal(1, #review_calls)
      local prompt: any = review_calls[1].prompt
      expect.truthy(prompt:find("### session bad", 1, true))
      expect.is_nil(prompt:find("### session clean", 1, true))
      expect.equal("2026-09-01T11:00:00Z", cru.storage.get("consolidation", "last_run"))

      plugin.run(nil)

      expect.equal(1, #review_calls)
    end)

    it("moves the cursor past a short session that follows the sample", function()
      -- `short` has one user turn and `min_turns` is 2, so it never
      -- qualifies. When every session that qualifies was taken, the cursor
      -- moves past the newest candidate, so no pass reads `short` again.
      cru.session.list = function()
        return {
          { id = "long", state = "ended", started_at = "2026-09-01T10:00:00Z" },
          { id = "short", state = "ended", started_at = "2026-09-01T11:00:00Z" },
        }, nil
      end

      plugin.run(nil)

      expect.equal(1, #review_calls)
      expect.equal("2026-09-01T11:00:00Z", cru.storage.get("consolidation", "last_run"))
    end)

    it("sends nothing on a second run when no session ended since the cursor", function()
      plugin.run(nil)
      plugin.run(nil)

      expect.equal(1, #review_calls)
    end)

    it("starts after the cursor a previous pass stored", function()
      plugin.setup({ min_turns = 1 })
      cru.storage.set("consolidation", "last_run", "2026-09-01T10:30:00Z")
      plugin.run(nil)

      expect.equal(1, #review_calls)
      local prompt: any = review_calls[1].prompt
      expect.truthy(prompt:find("### session long", 1, true))
      expect.is_nil(prompt:find("### session short", 1, true))
    end)

    it("a pass writes through its own session and stages nothing", function()
      -- The reviewer writes each note with the note tools in the pass's own
      -- session, so the pass hands nothing to a staging directory: the
      -- `stage_proposals` stub raises when a pass calls it. The title is what
      -- a human reads for the pass in the sessions list and in the Changes
      -- panel, so the pass names itself and the day it ran.
      review_text = "wrote Patterns/Retry after a tool error.md"
      cru.session.list = function()
        return { { id = "long", state = "ended", started_at = "2026-09-01T11:00:00Z" } }, nil
      end

      local summary = plugin.run(nil)

      expect.equal(1, #review_calls)
      expect.equal("Consolidation: " .. tostring(os.date("%Y-%m-%d")), review_calls[1].title)
      expect.equal("wrote Patterns/Retry after a tool error.md", summary)
    end)

    it("keeps the cursor where it was when a pass sees no session", function()
      cru.storage.set("consolidation", "last_run", "2026-09-01T12:00:00Z")
      plugin.run(nil)

      expect.equal("2026-09-01T12:00:00Z", cru.storage.get("consolidation", "last_run"))
      expect.equal(0, #review_calls)
    end)
  end)
end)
