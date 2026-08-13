--- Tests for the todo-list plugin.
---
--- Runs entirely against the `cru.fs` and `cru.paths` mocks — no real file is
--- read or written. The three regressions these lock down are the ones the
--- previous implementation had: rewriting the file destroyed its section
--- headings, filtered listings renumbered task ids so completing one hit the
--- wrong task, and paths resolved against the daemon's working directory.

-- Required by DIRECTORY NAME, never by `init`: the runner's package.path
-- mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.
local plugin = require("todo-list")

local TASKS_PATH = "/mock/kiln/TASKS.md"

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

local function with_file(content)
    test_mocks.setup({ fs = { files = { [TASKS_PATH] = content or SECTIONED } } })
end

local function written()
    local writes = test_mocks.get_calls("fs", "write")
    return writes[#writes] and writes[#writes][2]
end

describe("todo-list", function()
    before_each(function()
        test_mocks.setup()
        -- `config` is module state that survives require caching.
        plugin.setup({ default_file = "TASKS.md", show_completed = false })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("setup", function()
        it("applies the configured default file", function()
            plugin.setup({ default_file = "BACKLOG.md" })
            assert.equal(plugin.tools.tasks_list.fn({}).file, "/mock/kiln/BACKLOG.md")
        end)

        it("applies the configured show_completed default", function()
            with_file()
            plugin.setup({ show_completed = true })
            assert.equal(plugin.tools.tasks_list.fn({}).count, 3)
        end)

        it("defaults show_completed to false, as the manifest documents", function()
            with_file()
            assert.equal(plugin.tools.tasks_list.fn({}).count, 2)
        end)

        it("ignores a non-table config instead of erroring", function()
            plugin.setup(nil)
            plugin.setup(42)
            assert.equal(plugin.tools.tasks_list.fn({}).file, TASKS_PATH)
        end)
    end)

    describe("path resolution", function()
        it("resolves a relative file against the kiln, not the cwd", function()
            assert.equal(plugin.tools.tasks_list.fn({}).file, TASKS_PATH)
        end)

        it("uses an absolute file argument as given", function()
            assert.equal(plugin.tools.tasks_list.fn({ file = "/srv/T.md" }).file, "/srv/T.md")
        end)

        it("falls back to the workspace when no kiln is mounted", function()
            test_mocks.setup({ paths = { kiln = false } })
            assert.equal(plugin.tools.tasks_list.fn({}).file, "/mock/workspace/TASKS.md")
        end)
    end)

    describe("tasks_list", function()
        it("returns an empty list when the file does not exist", function()
            local result = plugin.tools.tasks_list.fn({})
            assert.equal(result.count, 0)
            assert.equal(result.total, 0)
            assert.deep_equal(result.tasks, {})
        end)

        it("parses text, completion and section for each task", function()
            with_file()
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            assert.equal(#tasks, 3)
            assert.equal(tasks[1].text, "write the parser")
            assert.equal(tasks[1].completed, false)
            assert.equal(tasks[1].section, "Now")
            assert.equal(tasks[2].text, "read the file")
            assert.equal(tasks[2].completed, true)
            assert.equal(tasks[3].section, "Later")
        end)

        it("accepts * bullets and an uppercase X", function()
            with_file("* [X] done\n* [ ] todo\n")
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            assert.equal(#tasks, 2)
            assert.equal(tasks[1].completed, true)
            assert.equal(tasks[2].completed, false)
        end)

        it("reports total separately from the filtered count", function()
            with_file()
            local result = plugin.tools.tasks_list.fn({ show_completed = false })
            assert.equal(result.count, 2)
            assert.equal(result.total, 3)
        end)

        it("keeps file-position ids when completed tasks are filtered out", function()
            with_file()
            local tasks = plugin.tools.tasks_list.fn({ show_completed = false }).tasks
            -- Task 2 is the completed one; the ids either side must NOT close
            -- the gap, or an id from this list completes the wrong task.
            assert.equal(tasks[1].id, 1)
            assert.equal(tasks[2].id, 3)
        end)

        it("writes nothing when only listing", function()
            with_file()
            plugin.tools.tasks_list.fn({})
            assert.equal(#test_mocks.get_calls("fs", "write"), 0)
        end)
    end)

    describe("tasks_add", function()
        it("rejects empty text", function()
            assert.truthy(plugin.tools.tasks_add.fn({ text = "" }).error)
        end)

        it("rejects nil text", function()
            assert.truthy(plugin.tools.tasks_add.fn({}).error)
        end)

        it("appends to the end of the file by default", function()
            with_file()
            local result = plugin.tools.tasks_add.fn({ text = "new thing" })
            assert.equal(result.success, true)
            assert.truthy(written():find("\n%- %[ %] new thing\n$"))
        end)

        it("preserves every heading and blank line", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "new thing" })
            local out = written()
            assert.truthy(out:find("# Tasks", 1, true))
            assert.truthy(out:find("## Now", 1, true))
            assert.truthy(out:find("## Later", 1, true))
            assert.truthy(out:find("Some prose that must survive an edit.", 1, true))
        end)

        it("leaves the original content byte-identical apart from the new line", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "new thing" })
            assert.equal(written(), SECTIONED .. "- [ ] new thing\n")
        end)

        it("files a task under a named section", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "and this", section = "Now" })
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            -- Straight after the last task already in "Now".
            assert.equal(tasks[3].text, "and this")
            assert.equal(tasks[3].section, "Now")
        end)

        it("appends when the named section does not exist", function()
            with_file()
            plugin.tools.tasks_add.fn({ text = "orphan", section = "Nowhere" })
            assert.truthy(written():find("\n%- %[ %] orphan\n$"))
        end)

        it("creates the file when it is missing", function()
            local result = plugin.tools.tasks_add.fn({ text = "first" })
            assert.equal(result.success, true)
            assert.equal(written(), "# Tasks\n\n- [ ] first\n")
        end)
    end)

    describe("tasks_complete", function()
        it("requires a task ID", function()
            assert.equal(plugin.tools.tasks_complete.fn({}).error, "Task ID is required")
        end)

        it("rejects non-numeric IDs", function()
            with_file()
            assert.truthy(plugin.tools.tasks_complete.fn({ id = "abc" }).error)
        end)

        it("rejects out-of-range IDs", function()
            with_file()
            assert.truthy(plugin.tools.tasks_complete.fn({ id = 999 }).error)
            assert.truthy(plugin.tools.tasks_complete.fn({ id = 0 }).error)
        end)

        it("reports a missing file rather than an invalid id", function()
            local result = plugin.tools.tasks_complete.fn({ id = 1 })
            assert.truthy(result.error:find("No tasks file", 1, true))
        end)

        it("marks the task and says which", function()
            with_file()
            local result = plugin.tools.tasks_complete.fn({ id = 1 })
            assert.equal(result.success, true)
            assert.equal(result.message, "Completed: write the parser")
        end)

        it("completes the task the id names, not the nth listed one", function()
            with_file()
            -- Id 3 is "ship it"; in a show_completed=false listing it is the
            -- SECOND row. Completing 3 must hit "ship it".
            plugin.tools.tasks_complete.fn({ id = 3 })
            local tasks = plugin.tools.tasks_list.fn({ show_completed = true }).tasks
            assert.equal(tasks[3].text, "ship it")
            assert.equal(tasks[3].completed, true)
            assert.equal(tasks[1].completed, false)
        end)

        it("touches only the one line", function()
            with_file()
            plugin.tools.tasks_complete.fn({ id = 1 })
            local expected = (SECTIONED:gsub("%- %[ %] write the parser", "- [x] write the parser", 1))
            assert.equal(written(), expected)
        end)

        it("does not re-complete an already completed task", function()
            with_file()
            local result = plugin.tools.tasks_complete.fn({ id = 2 })
            assert.equal(result.success, false)
            assert.equal(result.message, "Task already completed")
            assert.equal(#test_mocks.get_calls("fs", "write"), 0)
        end)

        it("does not rewrite a checkbox that appears in the task text", function()
            with_file("- [ ] fix the [ ] rendering\n")
            plugin.tools.tasks_complete.fn({ id = 1 })
            assert.equal(written(), "- [x] fix the [ ] rendering\n")
        end)
    end)

    describe("tasks_next", function()
        it("returns a message when there is no tasks file", function()
            local result = plugin.tools.tasks_next.fn({})
            assert.truthy(result.message)
            assert.equal(result.total, 0)
        end)

        it("returns the first uncompleted task in document order", function()
            with_file()
            local result = plugin.tools.tasks_next.fn({})
            assert.equal(result.id, 1)
            assert.equal(result.text, "write the parser")
            assert.equal(result.section, "Now")
        end)

        it("skips completed tasks", function()
            with_file("- [x] done\n- [ ] next up\n")
            assert.equal(plugin.tools.tasks_next.fn({}).text, "next up")
        end)

        it("counts only uncompleted tasks as remaining", function()
            with_file()
            -- Three tasks, one already done: after this one, one is left.
            assert.equal(plugin.tools.tasks_next.fn({}).remaining, 1)
        end)

        it("reports completion when every task is done", function()
            with_file("- [x] a\n- [x] b\n")
            local result = plugin.tools.tasks_next.fn({})
            assert.equal(result.message, "All tasks completed!")
            assert.equal(result.total, 2)
        end)
    end)

    describe("plugin metadata", function()
        it("exports the correct name", function()
            assert.equal(plugin.name, "todo-list")
        end)

        it("exports a version string", function()
            assert.equal(type(plugin.version), "string")
        end)

        it("exports a setup function so its config is applied", function()
            assert.equal(type(plugin.setup), "function")
        end)

        it("exports all expected tools", function()
            assert.truthy(plugin.tools.tasks_list)
            assert.truthy(plugin.tools.tasks_add)
            assert.truthy(plugin.tools.tasks_complete)
            assert.truthy(plugin.tools.tasks_next)
        end)

        it("exports the /tasks command", function()
            assert.truthy(plugin.commands.tasks)
            assert.equal(type(plugin.commands.tasks.fn), "function")
        end)
    end)
end)
