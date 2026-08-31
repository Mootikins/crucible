--!strict
describe("reflection", function()
  local plugin = require("reflection")

  describe("count_user_turns", function()
    it("counts only user messages", function()
      local msgs = {
        { role = "user", content = "hi" },
        { role = "assistant", content = "hello" },
        { role = "user", content = "again" },
        { role = "system", content = "sys" },
      }
      expect.equal(2, plugin.count_user_turns(msgs))
    end)

    it("handles empty and nil", function()
      expect.equal(0, plugin.count_user_turns({}))
      expect.equal(0, plugin.count_user_turns(nil))
    end)
  end)

  describe("build_transcript", function()
    it("renders role-labelled sections", function()
      local t = plugin.build_transcript({
        { role = "user", content = "question" },
        { role = "assistant", content = "answer" },
      })
      expect.truthy(t:find("## user"))
      expect.truthy(t:find("question"))
      expect.truthy(t:find("## assistant"))
      expect.truthy(t:find("answer"))
    end)
  end)

  describe("parse_proposals", function()
    it("parses a JSON array", function()
      local p = plugin.parse_proposals('[{"title":"T","body":"B"}]')
      expect.equal(1, #p)
      expect.equal("T", p[1].title)
    end)

    it("treats empty array as nothing to save", function()
      local p = plugin.parse_proposals("[]")
      expect.equal(0, #p)
    end)

    it("strips code fences", function()
      local p = plugin.parse_proposals('```json\n[{"title":"T","body":"B"}]\n```')
      expect.equal(1, #p)
    end)

    it("returns nil on non-JSON", function()
      expect.is_nil(plugin.parse_proposals("not json at all"))
    end)

    it("wraps a single proposal object into a one-element array", function()
      local p = plugin.parse_proposals('{"title":"T","body":"B"}')
      expect.equal(1, #p)
      expect.equal("T", p[1].title)
    end)

    it("treats empty string as empty list", function()
      expect.equal(0, #plugin.parse_proposals(""))
    end)
  end)

  describe("render_proposal", function()
    it("emits provenance frontmatter and body", function()
      local out = plugin.render_proposal(
        { title = "My Insight", body = "# Body\n\ntext", tags = { "learned" } },
        "chat-123",
        "2026-07-02T00:00:00Z"
      )
      expect.truthy(out:find("source: reflection"))
      expect.truthy(out:find("status: proposed"))
      expect.truthy(out:find("%[%[chat%-123%]%]"))
      expect.truthy(out:find('title: "My Insight"'))
      expect.truthy(out:find("  %- \"learned\""))
      expect.truthy(out:find("# Body"))
    end)

    it("includes target when provided", function()
      local out = plugin.render_proposal(
        { title = "T", body = "B", target = "Notes/x.md" }, "s", "t")
      expect.truthy(out:find('target: "Notes/x.md"'))
    end)

    it("omits tags block when none given", function()
      local out = plugin.render_proposal({ title = "T", body = "B" }, "s", "t")
      expect.falsy(out:find("tags:"))
    end)
  end)

  describe("proposal_id", function()
    it("is filesystem-safe and slugified", function()
      local id = plugin.proposal_id({ title = "Hello, World! & Co." }, 1, "20260702-000000")
      expect.falsy(id:find("[^%w%-]"))
      expect.truthy(id:find("hello"))
    end)

    it("falls back to 'note' for empty titles", function()
      local id = plugin.proposal_id({ title = "" }, 2, "20260702-000000")
      expect.truthy(id:find("note"))
    end)
  end)

  describe("is_reflection_session", function()
    it("detects the marker in the system prompt", function()
      local session = { system_prompt = plugin.reflection_marker .. "\n\nreview stuff" }
      expect.truthy(plugin.is_reflection_session(session))
    end)

    it("returns false for an ordinary session", function()
      expect.falsy(plugin.is_reflection_session({ system_prompt = "You are a helpful assistant" }))
    end)

    it("returns false when system_prompt is absent", function()
      expect.falsy(plugin.is_reflection_session({ id = "chat-1" }))
    end)

    it("returns false for nil", function()
      expect.falsy(plugin.is_reflection_session(nil))
    end)
  end)

  describe("run recursion guard", function()
    it("skips a session carrying the reflection marker without touching the daemon", function()
      -- The guard is the first thing run() checks, before any cru.session
      -- call, so a marked session short-circuits cleanly.
      local marked = { id = "aux-1", system_prompt = plugin.reflection_marker .. "\n\nx" }
      local ok = pcall(plugin.run, marked)
      expect.truthy(ok)
    end)
  end)

  describe("safe_filename", function()
    it("accepts a plain slug", function()
      expect.truthy(plugin.safe_filename("hello-world-1.md"))
    end)

    it("refuses traversal and hidden spellings", function()
      expect.falsy(plugin.safe_filename("../evil.md"))
      expect.falsy(plugin.safe_filename(".hidden.md"))
      expect.falsy(plugin.safe_filename("a/b.md"))
      expect.falsy(plugin.safe_filename(""))
    end)
  end)

  describe("run", function()
    local kiln_root

    before_each(function()
      -- The real bridge shape: `get_session` sends kiln NAMES in a `kilns`
      -- array, never a `kiln` path (session_bridge.rs).
      --
      -- `kiln_root` is a REAL directory: the plugin writes with `io.open`, so
      -- the staging directory has to exist. `real_dirs` makes the `mkdir` mock
      -- create it instead of only recording the call.
      kiln_root = os.tmpname()
      os.remove(kiln_root)
      test_mocks.setup({
        -- `cru.kiln.path` answers from this table, the way the daemon answers
        -- from the kiln registry.
        kiln = { roots = { notes = kiln_root } },
        fs = { real_dirs = true },
        sessions = {
          info = { id = "chat-1", session_type = "chat", state = "ended", kilns = { "notes" } },
          messages = {
            { role = "user", content = "question" },
            { role = "assistant", content = "answer" },
          },
          response_parts = {
            { type = "text", content = '[{"title":"T","body":"B"}]' },
          },
        },
      })
      plugin.setup({ model = "test-model", min_turns = 1 })
    end)

    after_each(function()
      test_mocks.reset()
    end)

    it("resolves the staging directory from the kiln the session names", function()
      plugin.run({ id = "chat-1" })

      -- The plugin asks for the directory by NAME. It never spells one.
      local asks = test_mocks.get_calls("kiln", "path")
      expect.equal(1, #asks)
      expect.equal("notes", asks[1][1])
      expect.equal(".crucible/proposals", asks[1][2])

      local staging = kiln_root .. "/.crucible/proposals"
      local mkdirs = test_mocks.get_calls("fs", "mkdir")
      expect.equal(staging, mkdirs[1] and mkdirs[1][1])
    end)

    it("writes the proposal file into the staging directory", function()
      local staged = plugin.stage_proposals("notes", "chat-1", {
        { title = "T", body = "B" },
      })

      expect.equal(1, #staged)
      local prefix = kiln_root .. "/.crucible/proposals/"
      expect.equal(prefix, staged[1]:sub(1, #prefix))
      expect.truthy(staged[1]:find("%.md$"))

      -- A REAL file, not a mock record: `io.open` has no mock to agree with.
      -- `assert` rather than `expect.truthy`: both fail the test when the
      -- file is missing, but only `assert` narrows the type, so the four
      -- lines below stop reading as calls on a possible nil.
      local handle = assert(io.open(staged[1], "r"))
      local body = assert(handle:read("a"))
      handle:close()
      expect.truthy((body:find("source: reflection")))
      expect.truthy((body:find("B")))
    end)

    it("raises when the session names a kiln that is not registered", function()
      test_mocks.setup({
        kiln = { roots = {} },
        sessions = {
          info = { id = "chat-1", session_type = "chat", state = "ended", kilns = { "gone" } },
        },
      })

      local ok = pcall(plugin.stage_proposals, "gone", "chat-1", {
        { title = "T", body = "B" },
      })
      expect.falsy(ok)
      expect.equal(0, #test_mocks.get_calls("fs", "mkdir"))
    end)
  end)

  describe("setup", function()
    it("accepts a config table", function()
      plugin.setup({ model = "test-model", min_turns = 1 })
      -- No error means the config merged cleanly.
      expect.truthy(true)
    end)
  end)
end)
