describe("reflection", function()
  local plugin = require("init")

  describe("count_user_turns", function()
    it("counts only user messages", function()
      local msgs = {
        { role = "user", content = "hi" },
        { role = "assistant", content = "hello" },
        { role = "user", content = "again" },
        { role = "system", content = "sys" },
      }
      assert.equal(plugin.count_user_turns(msgs), 2)
    end)

    it("handles empty and nil", function()
      assert.equal(plugin.count_user_turns({}), 0)
      assert.equal(plugin.count_user_turns(nil), 0)
    end)
  end)

  describe("build_transcript", function()
    it("renders role-labelled sections", function()
      local t = plugin.build_transcript({
        { role = "user", content = "question" },
        { role = "assistant", content = "answer" },
      })
      assert.truthy(t:find("## user"))
      assert.truthy(t:find("question"))
      assert.truthy(t:find("## assistant"))
      assert.truthy(t:find("answer"))
    end)
  end)

  describe("parse_proposals", function()
    it("parses a JSON array", function()
      local p = plugin.parse_proposals('[{"title":"T","body":"B"}]')
      assert.equal(#p, 1)
      assert.equal(p[1].title, "T")
    end)

    it("treats empty array as nothing to save", function()
      local p = plugin.parse_proposals("[]")
      assert.equal(#p, 0)
    end)

    it("strips code fences", function()
      local p = plugin.parse_proposals('```json\n[{"title":"T","body":"B"}]\n```')
      assert.equal(#p, 1)
    end)

    it("returns nil on non-JSON", function()
      assert.is_nil(plugin.parse_proposals("not json at all"))
    end)

    it("wraps a single proposal object into a one-element array", function()
      local p = plugin.parse_proposals('{"title":"T","body":"B"}')
      assert.equal(#p, 1)
      assert.equal(p[1].title, "T")
    end)

    it("treats empty string as empty list", function()
      assert.equal(#plugin.parse_proposals(""), 0)
    end)
  end)

  describe("render_proposal", function()
    it("emits provenance frontmatter and body", function()
      local out = plugin.render_proposal(
        { title = "My Insight", body = "# Body\n\ntext", tags = { "learned" } },
        "chat-123",
        "2026-07-02T00:00:00Z"
      )
      assert.truthy(out:find("source: reflection"))
      assert.truthy(out:find("status: proposed"))
      assert.truthy(out:find("%[%[chat%-123%]%]"))
      assert.truthy(out:find('title: "My Insight"'))
      assert.truthy(out:find("  %- \"learned\""))
      assert.truthy(out:find("# Body"))
    end)

    it("includes target when provided", function()
      local out = plugin.render_proposal(
        { title = "T", body = "B", target = "Notes/x.md" }, "s", "t")
      assert.truthy(out:find('target: "Notes/x.md"'))
    end)

    it("omits tags block when none given", function()
      local out = plugin.render_proposal({ title = "T", body = "B" }, "s", "t")
      assert.falsy(out:find("tags:"))
    end)
  end)

  describe("proposal_id", function()
    it("is filesystem-safe and slugified", function()
      local id = plugin.proposal_id({ title = "Hello, World! & Co." }, 1, "20260702-000000")
      assert.falsy(id:find("[^%w%-]"))
      assert.truthy(id:find("hello"))
    end)

    it("falls back to 'note' for empty titles", function()
      local id = plugin.proposal_id({ title = "" }, 2, "20260702-000000")
      assert.truthy(id:find("note"))
    end)
  end)

  describe("is_reflection_session", function()
    it("detects the marker in the system prompt", function()
      local session = { system_prompt = plugin.reflection_marker .. "\n\nreview stuff" }
      assert.truthy(plugin.is_reflection_session(session))
    end)

    it("returns false for an ordinary session", function()
      assert.falsy(plugin.is_reflection_session({ system_prompt = "You are a helpful assistant" }))
    end)

    it("returns false when system_prompt is absent", function()
      assert.falsy(plugin.is_reflection_session({ id = "chat-1" }))
    end)

    it("returns false for nil", function()
      assert.falsy(plugin.is_reflection_session(nil))
    end)
  end)

  describe("run recursion guard", function()
    it("skips a session carrying the reflection marker without touching the daemon", function()
      -- The guard is the first thing run() checks, before any cru.sessions
      -- call, so a marked session short-circuits cleanly.
      local marked = { id = "aux-1", system_prompt = plugin.reflection_marker .. "\n\nx" }
      local ok = pcall(plugin.run, marked)
      assert.truthy(ok)
    end)
  end)

  describe("setup", function()
    it("accepts a config table", function()
      plugin.setup({ model = "test-model", min_turns = 1 })
      -- No error means the config merged cleanly.
      assert.truthy(true)
    end)
  end)
end)
