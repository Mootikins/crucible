--- Tests for the daily-notes plugin.
---
--- Notes are REAL files: the plugin writes them with `io.open`, which has no
--- mock to agree with, so each test points the kiln mock at a fresh real
--- directory. The stray-file regression stays locked down: the previous
--- version of this suite called `daily_create` for a fixed date, which wrote
--- `Journal/2025-06-15.md` relative to the daemon's working directory, so no
--- test here ever resolves a writing call against the cwd.

-- Required by DIRECTORY NAME, never by `init`: the runner's package.path
-- mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.
local plugin = require("daily-notes")

--- Minted per test in before_each: a REAL kiln directory. `real_dirs` makes
--- the `cru.fs.mkdir` mock create directories for real, which the plugin's
--- own `mkdir` of the notes folder relies on too.
local KILN

--- What `path` holds on disk, or nil when it does not exist.
local function on_disk(path)
    local handle = io.open(path, "r")
    if not handle then return nil end
    local content = handle:read("a")
    handle:close()
    return content
end

describe("daily-notes", function()
    before_each(function()
        KILN = os.tmpname()
        os.remove(KILN)
        -- The plugin resolves the ACTIVE kiln by name through cru.kiln.path,
        -- so the fixture provides both the name and its root.
        test_mocks.setup({
            kiln = { active = "notes", roots = { notes = KILN } },
            fs = { real_dirs = true },
        })
        cru.fs.mkdir(KILN)
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
            expect.equal(result.path, KILN .. "/Diary/2025-06-15.md")
        end)

        it("applies the configured date format", function()
            plugin.setup({ date_format = "%Y%m%d" })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            expect.equal(result.path, KILN .. "/Journal/20250615.md")
        end)

        it("ignores a non-table config instead of erroring", function()
            plugin.setup(nil)
            plugin.setup("nonsense")
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            expect.equal(result.path, KILN .. "/Journal/2025-06-15.md")
        end)

        it("leaves unmentioned keys at their defaults", function()
            plugin.setup({ folder = "Diary" })
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            expect.equal(result.date, "2025-06-15")
        end)
    end)

    describe("path resolution", function()
        it("resolves a relative folder against the kiln, not the cwd", function()
            local result = plugin.tools.daily_open.fn({ date = "2025-06-15" })
            expect.equal(result.path, KILN .. "/Journal/2025-06-15.md")
        end)

        it("uses an absolute folder as given", function()
            -- daily_list resolves without writing: an absolute folder outside
            -- the temp kiln must not be created for real.
            plugin.setup({ folder = "/srv/journal" })
            local note = plugin.tools.daily_list.fn({ days = 1 }).notes[1]
            expect.equal(note.path, "/srv/journal/" .. os.date("%Y-%m-%d") .. ".md")
        end)

        it("falls back to the workspace when no kiln is active", function()
            -- daily_list resolves without writing, so the mock workspace path
            -- never has to exist for real.
            test_mocks.setup({})
            local note = plugin.tools.daily_list.fn({ days = 1 }).notes[1]
            expect.equal(note.path, "/mock/workspace/Journal/" .. os.date("%Y-%m-%d") .. ".md")
        end)

        it("falls back to a relative path when neither is configured", function()
            test_mocks.setup({ paths = { workspace = false } })
            local note = plugin.tools.daily_list.fn({ days = 1 }).notes[1]
            expect.equal(note.path, "Journal/" .. os.date("%Y-%m-%d") .. ".md")
        end)

        it("warns once, naming both paths, when only the legacy folder exists", function()
            -- The resolved journal directory is missing while the cwd-relative
            -- one exists — a silently empty journal without the warning. The
            -- mock's in-memory dirs stand in for the cwd, so no real file is
            -- involved.
            test_mocks.setup({
                kiln = { active = "notes", roots = { notes = "/kilns/notes" } },
                fs = { dirs = { ["Journal"] = true } },
            })
            local warnings = {}
            local had_log = cru.log
            ;(cru :: any).log = function(level, msg)
                if level == "warn" then warnings[#warnings + 1] = msg end
            end
            local ok, err = pcall(function()
                plugin.tools.daily_list.fn({ days = 1 })
                plugin.tools.daily_list.fn({ days = 1 })
            end)
            ;(cru :: any).log = had_log
            if not ok then error(err) end
            expect.equal(#warnings, 1)
            expect.truthy(warnings[1]:find("/kilns/notes/Journal", 1, true))
            expect.truthy(warnings[1]:find("'Journal'", 1, true))
        end)
    end)

    describe("daily_create", function()
        it("rejects invalid date formats", function()
            local result = plugin.tools.daily_create.fn({ date = "not-a-date" })
            expect.equal(result.error, "Invalid date format. Use YYYY-MM-DD")
        end)

        it("rejects a date that is nearly right", function()
            local result = plugin.tools.daily_create.fn({ date = "2025-6-15" })
            expect.equal(result.error, "Invalid date format. Use YYYY-MM-DD")
        end)

        it("writes the note and reports it created", function()
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            expect.falsy(result.error)
            expect.equal(result.created, true)
            expect.equal(result.path, KILN .. "/Journal/2025-06-15.md")
            expect.truthy(on_disk(result.path), "the note must be a real file")
        end)

        it("creates the notes directory before writing", function()
            plugin.tools.daily_create.fn({ date = "2025-06-15" })
            local mkdirs = test_mocks.get_calls("fs", "mkdir")
            expect.equal(mkdirs[#mkdirs][1], KILN .. "/Journal")
        end)

        it("writes the default body when no template is set", function()
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            expect.equal(on_disk(result.path), "# 2025-06-15\n\n## Notes\n\n## Tasks\n\n- [ ] \n")
        end)

        it("does not overwrite a note that already exists", function()
            cru.fs.mkdir(KILN .. "/Journal")
            local path = KILN .. "/Journal/2025-06-15.md"
            local handle = assert(io.open(path, "w"))
            handle:write("mine")
            handle:close()
            -- `exists` is the guard, and it answers from the mock: record the
            -- file there too, the way production `cru.fs.exists` would see it.
            test_mocks.setup({
                kiln = { active = "notes", roots = { notes = KILN } },
                fs = { real_dirs = true, files = { [path] = "mine" } },
            })
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            expect.equal(result.created, false)
            expect.equal(result.message, "Daily note already exists")
            expect.equal(on_disk(path), "mine")
        end)

        it("does not shift the date across a timezone boundary", function()
            -- Parsed at noon, not midnight: a midnight timestamp lands on the
            -- previous day wherever DST starts that morning.
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            expect.truthy(result.path:find("2025%-06%-15"))
        end)
    end)

    describe("templates", function()
        it("substitutes {{date}} and {{title}}", function()
            -- The template is read with `io.open`, so it is a real file too.
            local tpl = KILN .. "/tpl.md"
            local handle = assert(io.open(tpl, "w"))
            handle:write("# {{title}}\n\nlogged {{date}}\n")
            handle:close()
            plugin.setup({ template = tpl })

            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            expect.equal(on_disk(result.path), "# 2025-06-15\n\nlogged 2025-06-15\n")
        end)

        it("falls back to the default body when the template is missing", function()
            plugin.setup({ template = KILN .. "/nope.md" })
            local result = plugin.tools.daily_create.fn({ date = "2025-06-15" })
            expect.falsy(result.error)
            expect.equal(on_disk(result.path), "# 2025-06-15\n\n## Notes\n\n## Tasks\n\n- [ ] \n")
        end)
    end)

    describe("daily_open", function()
        it("returns a path and date for today", function()
            local result = plugin.tools.daily_open.fn({})
            expect.truthy(result.path)
            expect.truthy(result.date)
            expect.equal(result.created, true)
        end)

        it("reports created = false for a note that is already there", function()
            cru.fs.mkdir(KILN .. "/Journal")
            local path = KILN .. "/Journal/2025-03-20.md"
            local handle = assert(io.open(path, "w"))
            handle:write("hi")
            handle:close()
            test_mocks.setup({
                kiln = { active = "notes", roots = { notes = KILN } },
                fs = { real_dirs = true, files = { [path] = "hi" } },
            })
            local result = plugin.tools.daily_open.fn({ date = "2025-03-20" })
            expect.equal(result.created, false)
            expect.equal(on_disk(path), "hi")
        end)

        it("rejects an invalid date rather than silently using today", function()
            local result = plugin.tools.daily_open.fn({ date = "yesterday" })
            expect.equal(result.error, "Invalid date format. Use YYYY-MM-DD")
        end)
    end)

    describe("daily_list", function()
        it("returns the requested number of days", function()
            local result = plugin.tools.daily_list.fn({ days = 3 })
            expect.equal(result.count, 3)
            expect.equal(#result.notes, 3)
        end)

        it("defaults to 7 days", function()
            expect.equal(plugin.tools.daily_list.fn({}).count, 7)
        end)

        it("includes date, path and exists for each note", function()
            local note = plugin.tools.daily_list.fn({ days = 1 }).notes[1]
            expect.truthy(note.date)
            expect.truthy(note.path)
            expect.equal(type(note.exists), "boolean")
        end)

        it("reports exists = true only for notes on disk", function()
            local today = os.date("%Y-%m-%d")
            test_mocks.setup({
                kiln = { active = "notes", roots = { notes = KILN } },
                fs = { files = { [KILN .. "/Journal/" .. today .. ".md"] = "hi" } },
            })
            local notes = plugin.tools.daily_list.fn({ days = 2 }).notes
            expect.equal(notes[1].exists, true)
            expect.equal(notes[2].exists, false)
        end)

        it("rejects a non-positive day count", function()
            expect.truthy(plugin.tools.daily_list.fn({ days = 0 }).error)
            expect.truthy(plugin.tools.daily_list.fn({ days = -1 }).error)
        end)
    end)

    describe("plugin metadata", function()
        it("exports the correct name", function()
            expect.equal(plugin.name, "daily-notes")
        end)

        it("exports a version string", function()
            expect.equal(type(plugin.version), "string")
        end)

        it("exports a setup function so its config is applied", function()
            expect.equal(type(plugin.setup), "function")
        end)

        it("exports all expected tools", function()
            expect.truthy(plugin.tools.daily_create)
            expect.truthy(plugin.tools.daily_open)
            expect.truthy(plugin.tools.daily_list)
        end)

        it("exports the /daily command", function()
            expect.truthy(plugin.commands.daily)
            expect.equal(type(plugin.commands.daily.fn), "function")
        end)
    end)
end)
