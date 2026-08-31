--!strict
--- Tests for the todo-list plugin.
---
--- The tasks file is REAL: the plugin reads and writes it with `io.open`,
--- which has no mock to agree with, so each test gets a fresh directory and
--- the `cru.paths` mock points the kiln at it. The three regressions these
--- lock down are the ones the previous implementation had: rewriting the file
--- destroyed its section headings, filtered listings renumbered task ids so
--- completing one hit the wrong task, and paths resolved against the daemon's
--- working directory.

-- Required by DIRECTORY NAME, never by `init`: the runner's package.path
-- mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.
local plugin = require("todo-list")

--- Minted per test in before_each: a REAL kiln directory, and the tasks file
--- inside it. `real_dirs` makes the `cru.fs.mkdir` mock create it for real.
local KILN
local TASKS_PATH

--- A file with headings, prose, and a mix of done and not-done tasks — the
--- shape a whole-file rewrite silently flattens.
local SECTIONED = table.concat({
    "# Tasks",
    "",
    "Some prose that must survive an edit.",
    "",
    "## Now",
    "",
    "- [ ] write the parser",
    "- [x] read the file",
    "",
    "## Later",
    "",
    "- [ ] ship it",
}, "\n") .. "\n"

local function with_file(content: string?)
    local handle = assert(io.open(TASKS_PATH, "w"))
    handle:write(content or SECTIONED)
    handle:close()
end

--- What the tasks file holds right now, or nil when it does not exist.
local function written(): string?
    local handle = io.open(TASKS_PATH, "r")
    if not handle then return nil end
    local content = handle:read("a")
    handle:close()
    return content
end

describe("todo-list", function()
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
        TASKS_PATH = KILN .. "/TASKS.md"
        -- `config` is module state that survives require caching.
        plugin.setup({ default_file = "TASKS.md", show_completed = false })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("setup", function()
        it("applies the configured default file", function()
            plugin.setup({ default_file = "BACKLOG.md" })
            expect.equal(plugin.tools.tasks_list.fn({}).file, KILN .. "/BACKLOG.md")
        end)

        it("applies the configured show_completed default", function()
            with_file()
            plugin.setup({ show_completed = true })
            expect.equal(plugin.tools.tasks_list.fn({}).count, 3)
        end)

        it("defaults show_completed to false, as the manifest documents", function()
            with_file()
            expect.equal(plugin.tools.tasks_list.fn({}).count, 2)
        end)

        it("ignores a non-table config instead of erroring", function()
            plugin.setup(nil)
            plugin.setup(42)
            expect.equal(plugin.tools.tasks_list.fn({}).file, TASKS_PATH)
        end)
    end)

    describe("path resolution", function()
        it("resolves a relative file against the kiln, not the cwd", function()
            expect.equal(plugin.tools.tasks_list.fn({}).file, TASKS_PATH)
        end)

        it("uses an absolute file argument as given", function()
            expect.equal(plugin.tools.tasks_list.fn({ file = "/srv/T.md" }).file, "/srv/T.md")
        end)

        it("falls back to the workspace when no kiln is active", function()
            test_mocks.setup({})
            expect.equal(plugin.tools.tasks_list.fn({}).file, "/mock/workspace/TASKS.md")
        end)

        it("warns once, naming both paths, when only the legacy cwd file exists", function()
            -- The resolved location is missing and the old cwd-relative file
            -- is present — exactly the situation where a silently empty list
            -- would be the outcome. `cru.fs.exists` answers from the mock, so
            -- no real cwd file is involved.
            test_mocks.setup({
                kiln = { active = "notes", roots = { notes = "/kilns/notes" } },
                fs = { files = { ["TASKS.md"] = "- [ ] old task\n" } },
            })
            local warnings = {}
            local had_log = cru.log
            ;(cru :: any).log = function(level, msg)
                if level == "warn" then warnings[#warnings + 1] = msg end
            end
            local ok, err = pcall(function()
                plugin.tools.tasks_list.fn({})
                plugin.tools.tasks_list.fn({})
            end)
            ;(cru :: any).log = had_log
            if not ok then error(err) end
            expect.equal(#warnings, 1)
            expect.truthy(warnings[1]:find("/kilns/notes/TASKS.md", 1, true))
            expect.truthy(warnings[1]:find("'TASKS.md'", 1, true))
        end)
    end)

    describe("tasks_list", function()
        it("returns an empty list when the file does not exist", function()
            local result = plugin.tools.tasks_list.fn({})
            expect.equal(result.count, 0)
            expect.equal(result.total, 0)
            expect.deep_equal(result.tasks, {})
        end)

        it("parses text, completion and section for each task", function()
            with_file()
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            expect.equal(#tasks, 3)
            expect.equal(tasks[1].text, "write the parser")
            expect.equal(tasks[1].completed, false)
            expect.equal(tasks[1].section, "Now")
            expect.equal(tasks[2].text, "read the file")
            expect.equal(tasks[2].completed, true)
            expect.equal(tasks[3].section, "Later")
        end)

        it("accepts * bullets and an uppercase X", function()
            with_file("* [X] done\n* [ ] todo\n")
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            expect.equal(#tasks, 2)
            expect.equal(tasks[1].completed, true)
            expect.equal(tasks[2].completed, false)
        end)

        it("reports total separately from the filtered count", function()
            with_file()
            local result = plugin.tools.tasks_list.fn({ show_completed = false })
            expect.equal(result.count, 2)
            expect.equal(result.total, 3)
        end)

        it("keeps file-position ids when completed tasks are filtered out", function()
            with_file()
            local tasks = plugin.tools.tasks_list.fn({ show_completed = false }).tasks
            -- Task 2 is the completed one; the ids either side must NOT close
            -- the gap, or an id from this list completes the wrong task.
            expect.equal(tasks[1].id, 1)
            expect.equal(tasks[2].id, 3)
        end)

        it("writes nothing when only listing", function()
            with_file()
            plugin.tools.tasks_list.fn({})
            -- Byte-identical content is the point: a listing that rewrites the
            -- file is the regression that flattened section headings.
            expect.equal(assert(written()), SECTIONED)
        end)
    end)

    describe("tasks_add", function()
        it("rejects empty text", function()
            expect.truthy(plugin.tools.tasks_add.fn({ text = "" }).error)
        end)

        it("rejects nil text", function()
            expect.truthy(plugin.tools.tasks_add.fn({}).error)
        end)

        it("appends to the end of the file by default", function()
            with_file()
            local result = plugin.tools.tasks_add.fn({ text = "new thing" })
            expect.equal(result.success, true)
            expect.truthy((assert(written()):find("\n%- %[ %] new thing\n$")))
        end)

        it("preserves every heading and blank line", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "new thing" })
            local out = assert(written())
            expect.truthy((out:find("# Tasks", 1, true)))
            expect.truthy((out:find("## Now", 1, true)))
            expect.truthy((out:find("## Later", 1, true)))
            expect.truthy((out:find("Some prose that must survive an edit.", 1, true)))
        end)

        it("leaves the original content byte-identical apart from the new line", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "new thing" })
            expect.equal(assert(written()), SECTIONED .. "- [ ] new thing\n")
        end)

        it("files a task under a named section", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "and this", section = "Now" })
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            -- Straight after the last task already in "Now".
            expect.equal(tasks[3].text, "and this")
            expect.equal(tasks[3].section, "Now")
        end)

        it("appends when the named section does not exist", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "orphan", section = "Nowhere" })
            expect.truthy((assert(written()):find("\n%- %[ %] orphan\n$")))
        end)

        it("creates the file when it is missing", function()
            local result = plugin.tools.tasks_add.fn({ text = "first" })
            expect.equal(result.success, true)
            expect.equal(assert(written()), "# Tasks\n\n- [ ] first\n")
        end)
    end)

    describe("tasks_complete", function()
        it("requires a task ID", function()
            expect.equal(plugin.tools.tasks_complete.fn({}).error, "Task ID is required")
        end)

        it("rejects non-numeric IDs", function()
            with_file()
            expect.truthy(plugin.tools.tasks_complete.fn({ id = "abc" }).error)
        end)

        it("rejects out-of-range IDs", function()
            with_file()
            expect.truthy(plugin.tools.tasks_complete.fn({ id = 999 }).error)
            expect.truthy(plugin.tools.tasks_complete.fn({ id = 0 }).error)
        end)

        it("reports a missing file rather than an invalid id", function()
            local result = plugin.tools.tasks_complete.fn({ id = 1 })
            expect.truthy(result.error:find("No tasks file", 1, true))
        end)

        it("marks the task and says which", function()
            with_file()
            local result = plugin.tools.tasks_complete.fn({ id = 1 })
            expect.equal(result.success, true)
            expect.equal(result.message, "Completed: write the parser")
        end)

        it("completes the task the id names, not the nth listed one", function()
            with_file()
            -- Id 3 is "ship it"; in a show_completed=false listing it is the
            -- SECOND row. Completing 3 must hit "ship it".
            plugin.tools.tasks_complete.fn({ id = 3 })
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            expect.equal(tasks[3].text, "ship it")
            expect.equal(tasks[3].completed, true)
            expect.equal(tasks[1].completed, false)
        end)

        it("touches only the one line", function()
            with_file()
            plugin.tools.tasks_complete.fn({ id = 1 })
            local expected = (SECTIONED:gsub("%- %[ %] write the parser", "- [x] write the parser", 1))
            expect.equal(assert(written()), expected)
        end)

        it("does not re-complete an already completed task", function()
            with_file()
            local result = plugin.tools.tasks_complete.fn({ id = 2 })
            expect.equal(result.success, false)
            expect.equal(result.message, "Task already completed")
            expect.equal(assert(written()), SECTIONED)
        end)

        it("does not rewrite a checkbox that appears in the task text", function()
            with_file("- [ ] fix the [ ] rendering\n")
            plugin.tools.tasks_complete.fn({ id = 1 })
            expect.equal(assert(written()), "- [x] fix the [ ] rendering\n")
        end)
    end)

    describe("tasks_next", function()
        it("returns a message when there is no tasks file", function()
            local result = plugin.tools.tasks_next.fn({})
            expect.truthy(result.message)
            expect.equal(result.total, 0)
        end)

        it("returns the first uncompleted task in document order", function()
            with_file()
            local result = plugin.tools.tasks_next.fn({})
            expect.equal(result.id, 1)
            expect.equal(result.text, "write the parser")
            expect.equal(result.section, "Now")
        end)

        it("skips completed tasks", function()
            with_file("- [x] done\n- [ ] next up\n")
            expect.equal(plugin.tools.tasks_next.fn({}).text, "next up")
        end)

        it("counts only uncompleted tasks as remaining", function()
            with_file()
            -- Three tasks, one already done: after this one, one is left.
            expect.equal(plugin.tools.tasks_next.fn({}).remaining, 1)
        end)

        it("reports completion when every task is done", function()
            with_file("- [x] a\n- [x] b\n")
            local result = plugin.tools.tasks_next.fn({})
            expect.equal(result.message, "All tasks completed!")
            expect.equal(result.total, 2)
        end)
    end)

    describe("plugin metadata", function()
        it("exports the correct name", function()
            expect.equal(plugin.name, "todo-list")
        end)

        it("exports a version string", function()
            expect.equal(type(plugin.version), "string")
        end)

        it("exports a setup function so its config is applied", function()
            expect.equal(type(plugin.setup), "function")
        end)

        it("exports all expected tools", function()
            expect.truthy(plugin.tools.tasks_list)
            expect.truthy(plugin.tools.tasks_add)
            expect.truthy(plugin.tools.tasks_complete)
            expect.truthy(plugin.tools.tasks_next)
        end)

        it("exports the /tasks command", function()
            expect.truthy(plugin.commands.tasks)
            expect.equal(type(plugin.commands.tasks.fn), "function")
        end)
    end)
end)
