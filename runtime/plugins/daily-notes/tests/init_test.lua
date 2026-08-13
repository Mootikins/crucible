--- Tests for the daily-notes plugin.
---
--- Every case runs against the `cru.fs` and `cru.paths` mocks, so nothing here
--- touches a real filesystem. That is not merely tidiness: the previous
--- version of this suite called `daily_create` for a fixed date, which wrote
--- `Journal/2025-06-15.md` relative to the daemon's working directory — a
--- stray file in whatever tree the daemon happened to be started in.

-- Required by DIRECTORY NAME, never by `init`: the runner's package.path
-- mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.
local plugin = require("daily-notes")

describe("daily-notes", function()
    before_each(function()
        test_mocks.setup()
        -- Re-apply defaults: `config` is module state that survives require
        -- caching, so a test that changes `folder` would leak into the next.
        plugin.setup({ folder = "Journal", template = "", date_format = "%Y-%m-%d" })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("setup", function()
        it("applies the configured folder", function()
            plugin.setup({ folder = "Diary" })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "/mock/kiln/Diary/2025-06-15.md")
        end)

        it("applies the configured date format", function()
            plugin.setup({ date_format = "%Y%m%d" })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "/mock/kiln/Journal/20250615.md")
        end)

        it("ignores a non-table config instead of erroring", function()
            plugin.setup(nil)
            plugin.setup("nonsense")
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "/mock/kiln/Journal/2025-06-15.md")
        end)

        it("leaves unmentioned keys at their defaults", function()
            plugin.setup({ folder = "Diary" })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.date, "2025-06-15")
        end)
    end)

    describe("path resolution", function()
        it("resolves a relative folder against the kiln, not the cwd", function()
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "/mock/kiln/Journal/2025-06-15.md")
        end)

        it("uses an absolute folder as given", function()
            plugin.setup({ folder = "/srv/journal" })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "/srv/journal/2025-06-15.md")
        end)

        it("falls back to the workspace when no kiln is mounted", function()
            test_mocks.setup({ paths = { kiln = false } })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "/mock/workspace/Journal/2025-06-15.md")
        end)

        it("falls back to a relative path when neither is configured", function()
            test_mocks.setup({ paths = { kiln = false, workspace = false } })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            assert.equal(result.path, "Journal/2025-06-15.md")
        end)
    end)

    describe("daily_create", function()
        it("rejects invalid date formats", function()
            local result = plugin.tools.daily_create.fn({ date = "not-a-date" })
            assert.equal(result.error, "Invalid date format. Use YYYY-MM-DD")
        end)

        it("rejects a date that is nearly right", function()
            local result = plugin.tools.daily_create.fn({ date = "2025-6-15" })
            assert.equal(result.error, "Invalid date format. Use YYYY-MM-DD")
        end)

        it("writes the note and reports it created", function()
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            assert.falsy(result.error)
            assert.equal(result.created, true)
            assert.equal(result.path, "/mock/kiln/Journal/2025-06-15.md")

            local writes = test_mocks.get_calls("fs", "write")
            assert.equal(#writes, 1)
            assert.equal(writes[1][1], "/mock/kiln/Journal/2025-06-15.md")
        end)

        it("creates the notes directory before writing", function()
            plugin.tools.daily_create.fn({ date = "2025-06-15" })
            local mkdirs = test_mocks.get_calls("fs", "mkdir")
            assert.equal(#mkdirs, 1)
            assert.equal(mkdirs[1][1], "/mock/kiln/Journal")
        end)

        it("writes the default body when no template is set", function()
            plugin.tools.daily_create.fn({ date = "2025-06-15" })
            local body = test_mocks.get_calls("fs", "write")[1][2]
            assert.equal(body, "# 2025-06-15\n\n## Notes\n\n## Tasks\n\n- [ ] \n")
        end)

        it("does not overwrite a note that already exists", function()
            test_mocks.setup({
                fs = { files = { ["/mock/kiln/Journal/2025-06-15.md"] = "mine" } },
            })
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            assert.equal(result.created, false)
            assert.equal(result.message, "Daily note already exists")
            assert.equal(#test_mocks.get_calls("fs", "write"), 0)
        end)

        it("does not shift the date across a timezone boundary", function()
            -- Parsed at noon, not midnight: a midnight timestamp lands on the
            -- previous day wherever DST starts that morning.
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            assert.truthy(result.path:find("2025%-06%-15"))
        end)
    end)

    describe("templates", function()
        it("substitutes {{date}} and {{title}}", function()
            test_mocks.setup({
                fs = { files = { ["/tpl.md"] = "# {{title}}\n\nlogged {{date}}\n" } },
            })
            plugin.setup({ template = "/tpl.md" })

            plugin.tools.daily_create.fn({ date = "2025-06-15" })
            local body = test_mocks.get_calls("fs", "write")[1][2]
            assert.equal(body, "# 2025-06-15\n\nlogged 2025-06-15\n")
        end)

        it("falls back to the default body when the template is missing", function()
            plugin.setup({ template = "/nope.md" })
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            assert.falsy(result.error)
            local body = test_mocks.get_calls("fs", "write")[1][2]
            assert.equal(body, "# 2025-06-15\n\n## Notes\n\n## Tasks\n\n- [ ] \n")
        end)
    end)

    describe("daily_open", function()
        it("returns a path and date for today", function()
            local result = plugin.tools.daily_open.fn({})
            assert.truthy(result.path)
            assert.truthy(result.date)
            assert.equal(result.created, true)
        end)

        it("reports created = false for a note that is already there", function()
            test_mocks.setup({
                fs = { files = { ["/mock/kiln/Journal/2025-03-20.md"] = "hi" } },
            })
            local result = plugin.tools.daily_open.fn({ date = "2025-03-20" })
            assert.equal(result.created, false)
            assert.equal(#test_mocks.get_calls("fs", "write"), 0)
        end)

        it("rejects an invalid date rather than silently using today", function()
            local result = plugin.tools.daily_open.fn({ date = "yesterday" })
            assert.equal(result.error, "Invalid date format. Use YYYY-MM-DD")
        end)
    end)

    describe("daily_list", function()
        it("returns the requested number of days", function()
            local result = plugin.tools.daily_list.fn({ days = 3 })
            assert.equal(result.count, 3)
            assert.equal(#result.notes, 3)
        end)

        it("defaults to 7 days", function()
            assert.equal(plugin.tools.daily_list.fn({}).count, 7)
        end)

        it("includes date, path and exists for each note", function()
            local note = plugin.tools.daily_list.fn({ days = 1 }).notes[1]
            assert.truthy(note.date)
            assert.truthy(note.path)
            assert.equal(type(note.exists), "boolean")
        end)

        it("reports exists = true only for notes on disk", function()
            local today = os.date("%Y-%m-%d")
            test_mocks.setup({
                fs = { files = { ["/mock/kiln/Journal/" .. today .. ".md"] = "hi" } },
            })
            local notes = plugin.tools.daily_list.fn({ days = 2 }).notes
            assert.equal(notes[1].exists, true)
            assert.equal(notes[2].exists, false)
        end)

        it("rejects a non-positive day count", function()
            assert.truthy(plugin.tools.daily_list.fn({ days = 0 }).error)
            assert.truthy(plugin.tools.daily_list.fn({ days = -1 }).error)
        end)
    end)

    describe("plugin metadata", function()
        it("exports the correct name", function()
            assert.equal(plugin.name, "daily-notes")
        end)

        it("exports a version string", function()
            assert.equal(type(plugin.version), "string")
        end)

        it("exports a setup function so its config is applied", function()
            assert.equal(type(plugin.setup), "function")
        end)

        it("exports all expected tools", function()
            assert.truthy(plugin.tools.daily_create)
            assert.truthy(plugin.tools.daily_open)
            assert.truthy(plugin.tools.daily_list)
        end)

        it("exports the /daily command", function()
            assert.truthy(plugin.commands.daily)
            assert.equal(type(plugin.commands.daily.fn), "function")
        end)
    end)
end)
