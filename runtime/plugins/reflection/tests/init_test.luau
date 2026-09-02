--!strict
-- The runner VM has no `cru.on` (the daemon registers it), so the stub
-- records each registration. A test fires the handler it recorded.
local registered: { [string]: any } = {}
;(cru :: any).on = function(event: string, opts: any, fn: any)
  registered[event] = if type(opts) == "function" then opts else fn
end

--- Install a fake `cru.storage` (the runner VM has none) and answer with
--- its rows, keyed "<entity>/<key>". The caller runs `drop_storage()` when
--- it is done.
local function fake_storage(): { [string]: string }
  local rows: { [string]: string } = {}
  cru.storage = mock({
    set = function(entity: string, key: string, value: string)
      rows[entity .. "/" .. key] = value
      return true
    end,
    get = function(entity: string, key: string) return rows[entity .. "/" .. key] end,
    delete = function(entity: string, key: string)
      local had = rows[entity .. "/" .. key] ~= nil
      rows[entity .. "/" .. key] = nil
      return had
    end,
  })
  return rows
end

local function drop_storage()
  (cru :: any).storage = nil
end

describe("reflection", function()
  local plugin = require("reflection")
  local config = require("config")

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

  describe("build_transcript with tools", function()
    it("renders a tool call with its arguments and its result", function()
      -- The rows have different shapes, so the list is typed `any`.
      local msgs: { any } = {
        { role = "user", content = "run the tests" },
        { role = "tool_call", id = "c1", name = "bash", args = { command = "cargo test" } },
        { role = "tool_result", id = "c1", content = "ok" },
      }
      local t = plugin.build_transcript(msgs)
      expect.truthy(t:find("## tool_call bash", 1, true))
      expect.truthy(t:find("cargo test", 1, true))
      expect.truthy(t:find("## tool_result", 1, true))
      expect.truthy(t:find("ok", 1, true))
    end)

    it("marks a failed tool result", function()
      local t = plugin.build_transcript({
        { role = "tool_result", id = "c1", content = "", error = "exit 1" },
      })
      expect.truthy(t:find("ERROR: exit 1", 1, true))
    end)

    it("truncates a long tool result to tool_result_chars", function()
      local t = plugin.build_transcript({
        { role = "tool_result", id = "c1", content = string.rep("x", 5000) },
      }, { tool_result_chars = 100 })
      expect.truthy(#t < 400)
      expect.truthy(t:find("[truncated", 1, true))
    end)

    it("caps the whole transcript at transcript_chars, keeping the tail", function()
      local msgs = {}
      for i = 1, 50 do
        msgs[#msgs + 1] = { role = "user", content = "turn " .. i .. " " .. string.rep("y", 200) }
      end
      local t = plugin.build_transcript(msgs, { transcript_chars = 1000 })
      expect.truthy(#t <= 1100)
      expect.truthy(t:find("turn 50", 1, true))
      expect.is_nil(t:find("turn 1 ", 1, true))
    end)

    it("keeps every glyph whole when it cuts a tool result", function()
      -- Each `─` is three bytes. A byte cut at 10 lands inside a glyph.
      local t = plugin.build_transcript({
        { role = "tool_result", id = "c1", content = string.rep("─", 50) },
      }, { tool_result_chars = 10 })
      expect.truthy(utf8.len(t) ~= nil)
      expect.truthy(t:find("[truncated 40 chars]", 1, true))
    end)

    it("keeps every glyph whole when it cuts the transcript tail", function()
      -- The tail cut starts at a byte offset that splits a glyph.
      local t = plugin.build_transcript({
        { role = "user", content = string.rep("─", 100) },
      }, { transcript_chars = 100 })
      expect.truthy(utf8.len(t) ~= nil)
      expect.truthy(t:find("[earlier turns cut]", 1, true))
      expect.truthy(utf8.len(t) <= 100 + #"[earlier turns cut]\n\n")
    end)
  end)

  describe("build_transcript reads the caps from config", function()
    -- Reset on both sides: a cap an earlier suite set must not reach a
    -- case here, and a cap set here must not leak out.
    before_each(function()
      config.reset()
    end)

    after_each(function()
      config.reset()
    end)

    it("cuts a tool result at the configured tool_result_chars", function()
      config.init({ tool_result_chars = 10 })
      local t = plugin.build_transcript({
        { role = "tool_result", id = "c1", content = string.rep("x", 50) },
      })
      expect.truthy(t:find("[truncated 40 chars]", 1, true))
    end)

    it("cuts the transcript at the configured transcript_chars", function()
      config.init({ transcript_chars = 50 })
      local t = plugin.build_transcript({
        { role = "user", content = string.rep("y", 200) },
      })
      expect.truthy(t:find("[earlier turns cut]", 1, true))
      expect.truthy(#t <= 50 + #"[earlier turns cut]\n\n")
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

    it("records the model that wrote the proposal", function()
      local out = plugin.render_proposal(
        { title = "T", body = "B" }, "chat-1", "2026-09-01T00:00:00Z",
        { model = "claude-haiku-4-5-20251001" })
      expect.truthy(out:find('model: "claude-haiku-4-5-20251001"', 1, true))
    end)

    it("omits the model line when the session has none", function()
      local out = plugin.render_proposal({ title = "T", body = "B" }, "s", "t", {})
      expect.falsy(out:find("model:", 1, true))
    end)

    it("names the source the caller gives, and reflection by default", function()
      local out = plugin.render_proposal({ title = "T", body = "B" }, "s", "t", { source = "consolidation" })
      expect.truthy(out:find("source: consolidation", 1, true))
      expect.falsy(out:find("source: reflection", 1, true))
      local default = plugin.render_proposal({ title = "T", body = "B" }, "s", "t", {})
      expect.truthy(default:find("source: reflection", 1, true))
    end)

    it("omits the session line when there is no session", function()
      local out = plugin.render_proposal({ title = "T", body = "B" }, nil, "t", { source = "consolidation" })
      expect.falsy(out:find("session:", 1, true))
      expect.truthy(out:find("status: proposed", 1, true))
    end)

    it("escapes a double quote after the backslash, so a quoted value closes once", function()
      -- Title holds a backslash and a quote: a\"b. The backslash must be
      -- escaped first, or the quote escape gets a second backslash and the
      -- YAML string closes early.
      local out = plugin.render_proposal({ title = 'a\\"b', body = "B" }, "s", "t")
      expect.truthy(out:find('title: "a\\\\\\"b"', 1, true))
    end)

    it("escapes a line break and a backslash in every scalar, as the CLI does", function()
      local out = plugin.render_proposal(
        { title = "a\n---\nb", body = "B", target = "dir\\x.md", tags = { "t\r" } },
        "s", "t", { model = "m\nn" })
      expect.truthy(out:find('title: "a\\n---\\nb"', 1, true))
      expect.truthy(out:find('target: "dir\\\\x.md"', 1, true))
      expect.truthy(out:find('  - "t\\r"', 1, true))
      expect.truthy(out:find('model: "m\\nn"', 1, true))
      -- The frontmatter opens once and closes once: no scalar adds a fence line.
      local fences = 0
      for line in (out .. "\n"):gmatch("(.-)\n") do
        if line == "---" then fences += 1 end
      end
      expect.equal(2, fences)
    end)
  end)

  describe("validate_proposal", function()
    it("accepts a create with title and body", function()
      expect.is_nil(plugin.validate_proposal({ title = "T", body = "B" }))
    end)
    it("requires target on an update", function()
      expect.truthy(plugin.validate_proposal({ kind = "update", title = "T", body = "B" }))
    end)
    it("accepts an update that targets a skill", function()
      expect.is_nil(plugin.validate_proposal({ kind = "update", target = ".crucible/skills/x/SKILL.md", title = "x", body = "B" }))
      expect.is_nil(plugin.validate_proposal({ kind = "update", target = "Notes/SKILL.md", title = "x", body = "B" }))
    end)
    it("still refuses an update that targets the staging area", function()
      local reason = "update may not target the staging area"
      expect.equal(reason, plugin.validate_proposal({ kind = "update", target = ".crucible/proposals/y.md", title = "y", body = "B" }))
      expect.equal(reason, plugin.validate_proposal({ kind = "update", target = "./.crucible/proposals/y.md", title = "y", body = "B" }))
      -- A SKILL.md inside the staging area passes the `.crucible` rule, so
      -- only the staging rule refuses it.
      expect.equal(reason, plugin.validate_proposal({ kind = "update", target = ".crucible/proposals/x/SKILL.md", title = "x", body = "B" }))
    end)
    it("refuses an update to the rest of the .crucible directory, as the CLI does", function()
      expect.truthy(plugin.validate_proposal({ kind = "update", target = ".crucible/kiln.toml", title = "T", body = "B" }))
      expect.truthy(plugin.validate_proposal({ kind = "update", target = "Notes/.crucible/x.md", title = "T", body = "B" }))
      expect.truthy(plugin.validate_proposal({ kind = "update", target = "./.crucible/x.md", title = "T", body = "B" }))
    end)
    it("refuses a create on a skill path in every position, as the CLI does", function()
      -- `cru proposals accept` checks every path component, not only the first.
      expect.truthy(plugin.validate_proposal({ target = "Notes/.crucible/x.md", title = "T", body = "B" }))
      expect.truthy(plugin.validate_proposal({ target = "./.crucible/x.md", title = "T", body = "B" }))
      expect.truthy(plugin.validate_proposal({ target = ".crucible/skills/x/SKILL.md", title = "T", body = "B" }))
      expect.truthy(plugin.validate_proposal({ target = "Notes/SKILL.md", title = "T", body = "B" }))
      expect.truthy(plugin.validate_proposal({ target = "SKILL.md", title = "T", body = "B" }))
      -- A file that only contains the name is a plain note.
      expect.is_nil(plugin.validate_proposal({ title = "T", body = "B", target = "Notes/my.crucible.md" }))
      expect.is_nil(plugin.validate_proposal({ title = "T", body = "B", target = "Notes/NOT-SKILL.md" }))
    end)
    it("refuses a skill description that spans lines", function()
      -- The CLI refuses it at accept time: the frontmatter splitter cuts at
      -- any line that starts with `---`.
      expect.truthy(plugin.validate_proposal({ kind = "skill", body = "B", name = "x", description = "a\nb" }))
      expect.truthy(plugin.validate_proposal({ kind = "skill", body = "B", name = "x", description = "a\rb" }))
    end)
    it("requires name and description on a skill", function()
      expect.truthy(plugin.validate_proposal({ kind = "skill", body = "B", name = "x" }))
      expect.is_nil(plugin.validate_proposal({ kind = "skill", body = "B", name = "x", description = "d" }))
      expect.is_nil(plugin.validate_proposal({ kind = "skill", body = "B", name = "fix-flaky-nextest", description = "d" }))
      for _, bad in ipairs({ "-lead", "trail-", "two--hyphens", "Upper", "has space", string.rep("x", 65) }) do
        expect.truthy(plugin.validate_proposal({ kind = "skill", body = "B", name = bad, description = "d" }))
      end
    end)
    it("rejects an unknown kind", function()
      expect.truthy(plugin.validate_proposal({ kind = "patch", title = "T", body = "B" }))
    end)
  end)

  describe("render_proposal kinds", function()
    it("writes kind, name and description for a skill", function()
      local out = plugin.render_proposal(
        { kind = "skill", name = "fix-x", description = "d", body = "B" }, "s", "2026-09-01T00:00:00Z"
      )
      expect.truthy(out:find("kind: skill", 1, true))
      expect.truthy(out:find("name: fix-x", 1, true))
      expect.truthy(out:find('description: "d"', 1, true))
    end)
    it("writes kind and target for an update", function()
      local out = plugin.render_proposal(
        { kind = "update", target = "Notes/a.md", title = "A", body = "B" }, "s", "2026-09-01T00:00:00Z"
      )
      expect.truthy(out:find("kind: update", 1, true))
      expect.truthy(out:find('target: "Notes/a.md"', 1, true))
    end)
    it("omits the kind line for a create", function()
      local out = plugin.render_proposal({ title = "T", body = "B" }, "s", "t")
      expect.falsy(out:find("kind:", 1, true))
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

    it("slugifies the name of a skill that has no title", function()
      local id = plugin.proposal_id({ kind = "skill", name = "fix-x" }, 3, "20260702-000000")
      expect.truthy(id:find("fix-x", 1, true))
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

    --- Install the daemon mocks for one ended session that names the
    --- `notes` kiln. `fs` is the fixture for the `cru.fs` mock; a test that
    --- needs a file on the listing passes its own.
    local function mock_daemon(fs: any)
      test_mocks.setup({
        -- `cru.kiln.path` answers from this table, the way the daemon answers
        -- from the kiln registry.
        kiln = { roots = { notes = kiln_root } },
        fs = fs,
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
    end

    before_each(function()
      -- The real bridge shape: `get_session` sends kiln NAMES in a `kilns`
      -- array, never a `kiln` path (session_bridge.rs).
      --
      -- `kiln_root` is a REAL directory: the plugin writes with `io.open`, so
      -- the staging directory has to exist. `real_dirs` makes the `mkdir` mock
      -- create it instead of only recording the call.
      kiln_root = os.tmpname()
      os.remove(kiln_root)
      mock_daemon({ real_dirs = true })
      -- Start from the defaults: `config.init` merges keys and cannot unset
      -- one, so a value from an earlier test would leak into this one.
      config.reset()
      plugin.setup({ model = "test-model", min_turns = 1 })
    end)

    after_each(function()
      -- Take the real directory away before `reset()` swaps the mock for
      -- one that no longer reaches the disk. A case that staged nothing
      -- made no directory, and the pcall absorbs that.
      pcall(cru.fs.remove_all, kiln_root)
      test_mocks.reset()
      config.reset()
    end)

    it("resolves the staging directory from the kiln the session names", function()
      plugin.run({ id = "chat-1" })

      -- The plugin asks for each directory by NAME. It never spells one.
      -- The rejected directory comes first: the reviewer reads it before
      -- it proposes. The staging directory comes when the proposals land.
      local asks = test_mocks.get_calls("kiln", "path")
      expect.equal(2, #asks)
      expect.equal("notes", asks[1][1])
      expect.equal(".crucible/proposals/rejected", asks[1][2])
      expect.equal("notes", asks[2][1])
      expect.equal(".crucible/proposals", asks[2][2])

      local staging = kiln_root .. "/.crucible/proposals"
      local mkdirs = test_mocks.get_calls("fs", "mkdir")
      expect.equal(staging, mkdirs[1] and mkdirs[1][1])
    end)

    it("tells the reviewer the title of a proposal a human rejected", function()
      local rejected_dir = kiln_root .. "/.crucible/proposals/rejected"
      local rejected_file = rejected_dir .. "/reflection-20260901-100000-1-socket.md"

      -- The plugin finds the file through the `cru.fs` mocks, so the fixture
      -- registers the directory and the file name. The mock content is a
      -- decoy: the plugin must read the title from the disk with `io.open`.
      mock_daemon({
        real_dirs = true,
        dirs = { [rejected_dir] = true },
        files = { [rejected_file] = "decoy: the plugin must not read the mock" },
      })
      cru.fs.mkdir(rejected_dir)
      local handle = assert(io.open(rejected_file, "w"))
      handle:write("---\ntitle: Socket rules\nstatus: rejected\n---\nBody")
      handle:close()

      plugin.run({ id = "chat-1" })

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      local prompt = sent[1][2]
      expect.truthy(prompt:find("do not propose", 1, true))
      expect.truthy(prompt:find("- Socket rules", 1, true))
      expect.is_nil(prompt:find("decoy", 1, true))
    end)

    it("tells the reviewer which notes precognition injected, then forgets them", function()
      local rows = fake_storage()
      plugin.record_injected("chat-1", {
        results = { { index = 1, title = "Socket rules", score = 0.9 } },
      })

      plugin.run({ id = "chat-1" })

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      expect.truthy(sent[1][2]:find("already given", 1, true))
      expect.truthy(sent[1][2]:find("- Socket rules", 1, true))
      expect.is_nil(rows["chat-1/injected_titles"])
      drop_storage()
    end)

    it("asks the daemon for the tool rows of the session", function()
      plugin.run({ id = "chat-1" })

      local asked = test_mocks.get_calls("session", "messages")
      expect.equal(1, #asked)
      expect.equal("chat-1", asked[1][1])
      expect.equal(true, asked[1][2].tools)
    end)

    it("sends a prompt with no rejection block when nothing was rejected", function()
      plugin.run({ id = "chat-1" })

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      expect.is_nil(sent[1][2]:find("do not propose", 1, true))
    end)

    it("configures the reviewer with the declared max_iterations", function()
      plugin.run({ id = "chat-1" })

      -- The mock records the config table `M.run` hands to the daemon. The
      -- assertion reads the declared key, so a read of a stale name fails.
      local configured = test_mocks.get_calls("session", "configure_agent")
      expect.equal(1, #configured)
      expect.equal(12, configured[1][2].max_iterations)
    end)

    it("hands a raised max_iterations to the reviewer", function()
      plugin.setup({ max_iterations = 20 })
      plugin.run({ id = "chat-1" })

      local configured = test_mocks.get_calls("session", "configure_agent")
      expect.equal(20, configured[1][2].max_iterations)
    end)

    it("attaches the kiln to the reviewer's session", function()
      plugin.run({ id = "chat-1" })

      -- The reviewer reads the kiln with semantic_search and read_note, so
      -- the aux session names the same kiln as the session that ended.
      local created = test_mocks.get_calls("session", "create")
      expect.equal(1, #created)
      expect.equal("notes", created[1][1].kilns[1])
    end)

    it("creates the reviewer as a plugin session", function()
      plugin.run({ id = "chat-1" })

      -- The daemon knows the type. That is what the recursion guard reads,
      -- so the create call is the one place the type is set.
      local created = test_mocks.get_calls("session", "create")
      expect.equal(1, #created)
      expect.equal("plugin", created[1][1].type)
    end)

    it("bakes no marker into the reviewer's prompt", function()
      plugin.run({ id = "chat-1" })

      local configured = test_mocks.get_calls("session", "configure_agent")
      expect.equal(1, #configured)
      local prompt: any = configured[1][2].system_prompt
      expect.is_nil(prompt:find("crucible:reflection-pass", 1, true))
      expect.is_nil(prompt:find("<!--", 1, true))
    end)

    it("skips a plugin session and never creates a reviewer", function()
      -- The aux session ends too, and its end fires on_session_end again.
      -- The guard reads the type the daemon reports, not a prompt marker.
      test_mocks.setup({
        kiln = { roots = { notes = kiln_root } },
        fs = { real_dirs = true },
        sessions = {
          info = { id = "plugin-1", session_type = "plugin", state = "ended", kilns = { "notes" } },
          messages = {
            { role = "user", content = "review this" },
            { role = "assistant", content = "[]" },
          },
          response_parts = { { type = "text", content = "[]" } },
        },
      })

      local written = plugin.run({ id = "plugin-1" })

      expect.equal(0, #written)
      expect.equal(0, #test_mocks.get_calls("session", "create"))
      expect.equal(0, #test_mocks.get_calls("session", "send_and_collect"))
    end)

    it("gives the reviewer the session's outcome evidence", function()
      -- The mock has no review ledger; a test that wants hunks supplies them.
      (cru.session :: any).review_list_hunks = function(id)
        return { { state = "accepted" }, { state = "rejected" } }, nil
      end

      plugin.run({ id = "chat-1" })

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      local prompt = sent[1][2] :: string
      expect.truthy(prompt:find("user turns: 1", 1, true) ~= nil)
      expect.truthy(prompt:find("edits rejected: 1", 1, true) ~= nil)
    end)

    it("still reviews a session when review_list_hunks fails", function()
      (cru.session :: any).review_list_hunks = function(id)
        error("no ledger")
      end

      plugin.run({ id = "chat-1" })

      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal(1, #sent)
      local prompt = sent[1][2] :: string
      expect.truthy(prompt:find("edits rejected: 0", 1, true) ~= nil)
    end)

    it("skips a proposal that fails validation", function()
      -- The two proposals have different shapes, so the list is typed `any`.
      local proposals: { any } = {
        { kind = "update", title = "T", body = "B" },
        { kind = "skill", name = "fix-x", description = "d", body = "B" },
      }
      local staged = plugin.stage_proposals("notes", "chat-1", proposals)

      expect.equal(1, #staged)
      expect.truthy(staged[1]:find("fix-x", 1, true))
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

    it("tells the user how many proposals it staged and how to review them", function()
      local staged = plugin.stage_proposals("notes", "chat-1", {
        { title = "T", body = "B" },
        { title = "U", body = "C" },
      })

      expect.equal(2, #staged)
      local notified = test_mocks.get_calls("log", "notify")
      expect.equal(1, #notified)
      local message = notified[1][1] :: string
      expect.truthy(message:find("2 proposal", 1, true) ~= nil)
      expect.truthy(message:find("cru proposals list", 1, true) ~= nil)
    end)

    it("names the caller's source in the toast", function()
      local staged = plugin.stage_proposals("notes", nil, {
        { title = "T", body = "B" },
      }, { source = "consolidation" })

      expect.equal(1, #staged)
      local notified = test_mocks.get_calls("log", "notify")
      expect.equal(1, #notified)
      local message = notified[1][1] :: string
      expect.truthy(message:find("^consolidation: 1 proposal") ~= nil)
    end)

    it("says nothing to the user when it staged nothing", function()
      local staged = plugin.stage_proposals("notes", "chat-1", {
        { kind = "update", title = "T", body = "B" },
      })

      expect.equal(0, #staged)
      expect.equal(0, #test_mocks.get_calls("log", "notify"))
    end)

    it("writes the source the caller names and no session line without a session", function()
      local staged = plugin.stage_proposals("notes", nil, {
        { title = "T", body = "B" },
      }, { source = "consolidation" })

      expect.equal(1, #staged)
      local handle = assert(io.open(staged[1], "r"))
      local body = assert(handle:read("a")) :: string
      handle:close()
      expect.truthy(body:find("source: consolidation", 1, true) ~= nil)
      expect.truthy(body:find("session:", 1, true) == nil)
    end)

    it("records the model of the session that ended in the staged file", function()
      test_mocks.setup({
        kiln = { roots = { notes = kiln_root } },
        fs = { real_dirs = true },
        sessions = {
          info = {
            id = "chat-1", session_type = "chat", state = "ended",
            kilns = { "notes" }, model = "claude-haiku-4-5-20251001",
          },
          messages = {
            { role = "user", content = "question" },
            { role = "assistant", content = "answer" },
          },
          response_parts = {
            { type = "text", content = '[{"title":"T","body":"B"}]' },
          },
        },
      })

      local staged = plugin.run({ id = "chat-1" })

      expect.equal(1, #staged)
      local handle = assert(io.open(staged[1], "r"))
      local body = assert(handle:read("a"))
      handle:close()
      expect.truthy((body:find('model: "claude-haiku-4-5-20251001"', 1, true)))
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

  describe("review", function()
    --- The options one review takes. A test overrides what it asserts on.
    local function opts(): any
      return {
        kiln = "notes", model = "test-model", system_prompt = "be brief",
        prompt = "review this", timeout = 5, max_iterations = 3,
      }
    end

    before_each(function()
      test_mocks.setup({
        kiln = { roots = { notes = "/mock/notes" } },
        sessions = {
          response_parts = {
            { type = "text", content = "[" },
            { type = "text", content = "]" },
          },
        },
      })
    end)

    after_each(function()
      test_mocks.reset()
    end)

    it("answers with the reviewer's text and ends the aux session", function()
      local text, err = plugin.review(opts())

      expect.equal("[]", text)
      expect.is_nil(err)
      local created = test_mocks.get_calls("session", "create")
      expect.equal(1, #created)
      expect.equal("plugin", created[1][1].type)
      expect.equal("notes", created[1][1].kilns[1])
      local configured = test_mocks.get_calls("session", "configure_agent")
      expect.equal("be brief", configured[1][2].system_prompt)
      expect.equal(3, configured[1][2].max_iterations)
      local sent = test_mocks.get_calls("session", "send_and_collect")
      expect.equal("review this", sent[1][2])
      expect.equal(5, sent[1][3].timeout)
      local ended = test_mocks.get_calls("session", "end_session")
      expect.equal(1, #ended)
      expect.equal("mock-session-1", ended[1][1])
    end)

    it("narrows the reviewer to tools that only read the kiln", function()
      plugin.review(opts())

      -- The set is a list of glob patterns, enforced at dispatch. A write
      -- tool in it, or a wildcard, would let the reviewer write to the
      -- kiln or the workspace, and the contract is "nothing auto-merges".
      local narrowed = test_mocks.get_calls("tools", "set_active")
      expect.equal(1, #narrowed)
      expect.equal("mock-session-1", narrowed[1][1])
      local names: { string } = narrowed[1][2]
      expect.truthy(#names > 0)
      for _, name in ipairs(names) do
        expect.truthy(name:find("*", 1, true) == nil)
        expect.truthy(name ~= "create_note")
        expect.truthy(name ~= "update_note")
        expect.truthy(name ~= "write_file")
        expect.truthy(name ~= "bash")
      end
    end)

    it("sends nothing when the daemon refuses to narrow the tool set", function()
      (cru.tools :: any).set_active = function(id, patterns)
        return nil, "active tool sets are not bound"
      end

      local text, err = plugin.review(opts())

      expect.is_nil(text)
      expect.truthy((err :: string):find("not bound", 1, true) ~= nil)
      expect.equal(0, #test_mocks.get_calls("session", "send_and_collect"))
      expect.equal(1, #test_mocks.get_calls("session", "end_session"))
    end)

    it("ends the aux session when the review raises", function()
      cru.session.send_and_collect = function(id, prompt, opts)
        error("boom")
      end

      local text, err = plugin.review(opts())

      expect.is_nil(text)
      expect.truthy(err)
      local ended = test_mocks.get_calls("session", "end_session")
      expect.equal(1, #ended)
      expect.equal("mock-session-1", ended[1][1])
    end)

    it("answers with the error when the daemon refuses the send", function()
      cru.session.send_and_collect = function(id, prompt, opts)
        return nil, "no such model"
      end

      local text, err = plugin.review(opts())

      expect.is_nil(text)
      expect.truthy((err :: string):find("no such model", 1, true) ~= nil)
      expect.equal(1, #test_mocks.get_calls("session", "end_session"))
    end)
  end)

  describe("setup", function()
    it("accepts a config table", function()
      plugin.setup({ model = "test-model", min_turns = 1 })
      -- No error means the config merged cleanly.
      expect.truthy(true)
    end)
  end)

  describe("config", function()
    -- Every test here reads the `defaults` layer, so clear the setup layer
    -- on both sides. A value an earlier test wrote must not replace the
    -- default.
    before_each(function()
      config.reset()
    end)

    after_each(function()
      config.reset()
    end)

    it("declares max_iterations with a default of 12", function()
      expect.equal(12, config.get("max_iterations"))
    end)

    it("declares the two transcript caps", function()
      expect.equal(2000, config.get("tool_result_chars"))
      expect.equal(60000, config.get("transcript_chars"))
    end)

    it("lets setup() raise max_iterations", function()
      config.init({ max_iterations = 20 })
      expect.equal(20, config.get("max_iterations"))
    end)

    it("returns to the default after reset()", function()
      config.init({ max_iterations = 20 })
      config.reset()
      expect.equal(12, config.get("max_iterations"))
    end)
  end)

  describe("rejected_titles", function()
    it("returns the newest titles first, capped at n", function()
      local files = {
        ["reflection-20260901-100000-1-a.md"] = "---\ntitle: Socket rules\n---\nB",
        ["reflection-20260901-100200-1-c.md"] = "---\ntitle: \"Daemon flags\"\n---\nB",
        ["reflection-20260901-100300-1-d.md"] = "---\ntitle: Old one\n---\nB",
        ["notes.txt"] = "not a proposal",
      }
      local got = plugin.rejected_titles(files, 2)
      expect.equal(2, #got)
      expect.equal("Old one", got[1])
      expect.equal("Daemon flags", got[2])
    end)

    it("returns an empty list when there is nothing", function()
      expect.equal(0, #plugin.rejected_titles({}, 5))
      expect.equal(0, #plugin.rejected_titles(nil, 5))
    end)
  end)

  describe("build_prompt", function()
    it("names rejected titles the reviewer must not repeat", function()
      local p = plugin.build_prompt({
        transcript = "## user\nhi",
        rejected = { "Socket rules" },
      })
      expect.truthy(p:find("Socket rules", 1, true))
      expect.truthy(p:find("do not propose", 1, true))
    end)

    it("omits the rejection block when nothing was rejected", function()
      local p = plugin.build_prompt({ transcript = "t", rejected = {} })
      expect.is_nil(p:find("do not propose", 1, true))
    end)
  end)

  describe("injected_titles", function()
    it("collects titles from a precognition_select event", function()
      local got = plugin.injected_titles({
        results = {
          { index = 1, title = "Socket rules", score = 0.9 },
          { index = 2, title = "Kilns", score = 0.8 },
        },
      })
      expect.equal(2, #got)
      expect.equal("Socket rules", got[1])
      expect.equal("Kilns", got[2])
    end)

    it("returns an empty list for a missing or malformed event", function()
      expect.equal(0, #plugin.injected_titles(nil))
      expect.equal(0, #plugin.injected_titles({}))
      expect.equal(0, #plugin.injected_titles({ results = { { index = 1, score = 0.5 } } }))
    end)
  end)

  describe("precognition_select observer", function()
    it("records the injected titles under the session id and observes only", function()
      local rows = fake_storage()
      local handler = registered["precognition_select"]
      expect.truthy(handler)

      local out = handler({ session_id = "chat-9" }, {
        results = { { index = 1, title = "Socket rules", score = 0.9 } },
      })

      expect.is_nil(out)
      local stored = cru.json.decode(rows["chat-9/injected_titles"])
      expect.equal(1, #stored)
      expect.equal("Socket rules", stored[1])
      expect.equal(1, #plugin.read_injected("chat-9"))
      drop_storage()
    end)

    it("records nothing without a session id", function()
      local rows = fake_storage()
      registered["precognition_select"]({}, { results = { { index = 1, title = "X" } } })
      expect.is_nil(next(rows))
      drop_storage()
    end)
  end)

  describe("build_prompt with injected notes", function()
    it("names notes the agent was already given", function()
      local p = plugin.build_prompt({ transcript = "t", injected = { "Socket rules" } })
      expect.truthy(p:find("already given", 1, true))
      expect.truthy(p:find("- Socket rules", 1, true))
    end)

    it("omits the block when precognition injected nothing", function()
      local p = plugin.build_prompt({ transcript = "t", injected = {} })
      expect.is_nil(p:find("already given", 1, true))
    end)
  end)

  describe("build_outcome", function()
    it("counts turns, tool calls, tool errors and review decisions", function()
      -- The rows have different shapes, so the list is typed `any`.
      local messages: { any } = {
        { role = "user", content = "a" },
        { role = "tool_call", name = "bash" },
        { role = "tool_result", content = "", error = "exit 1" },
        { role = "tool_call", name = "bash" },
        { role = "tool_result", content = "ok" },
        { role = "user", content = "b" },
      }
      local o = plugin.build_outcome(messages, {
        { state = "accepted" }, { state = "rejected" }, { state = "rejected" },
      })
      expect.equal(2, o.user_turns)
      expect.equal(2, o.tool_calls)
      expect.equal(1, o.tool_errors)
      expect.equal(1, o.hunks_accepted)
      expect.equal(2, o.hunks_rejected)
    end)

    it("counts the edits nobody reviewed yet", function()
      -- `unreviewed` is the serde default of ReviewState, so a hunk with no
      -- state counts there too.
      local hunks: { any } = { { state = "unreviewed" }, {}, { state = "accepted" } }
      local o = plugin.build_outcome({}, hunks)
      expect.equal(2, o.hunks_unreviewed)
      expect.equal(1, o.hunks_accepted)
    end)

    it("renders as a short block the prompt can carry", function()
      local text = plugin.render_outcome({ user_turns = 3, tool_calls = 4, tool_errors = 1, hunks_accepted = 0, hunks_rejected = 2, hunks_unreviewed = 3 })
      expect.truthy(text:find("tool errors: 1", 1, true))
      expect.truthy(text:find("edits rejected: 2", 1, true))
      expect.truthy(text:find("edits not yet reviewed: 3", 1, true))
    end)
  end)

  describe("build_prompt with an outcome block", function()
    it("places the outcome before the transcript", function()
      local p = plugin.build_prompt({ transcript = "## user\nhi", outcome = "Session outcome evidence:\n- tool errors: 1" })
      local outcome_at = p:find("Session outcome evidence:", 1, true)
      local transcript_at = p:find("## user", 1, true)
      expect.truthy(outcome_at)
      expect.truthy(transcript_at)
      expect.truthy(outcome_at < transcript_at)
    end)

    it("omits the block when no outcome is given", function()
      local p = plugin.build_prompt({ transcript = "t" })
      expect.is_nil(p:find("Session outcome evidence:", 1, true))
    end)
  end)
end)
