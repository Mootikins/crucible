--!strict
--- todo-list — manage a markdown checklist file.
---
--- Two things this plugin gets right that the obvious implementation does not:
---
--- 1. **Edits are line-local.** The previous version re-emitted the whole file
---    from the parsed task list, which silently deleted every section heading,
---    every blank line, and every paragraph between them — the parser reads
---    `## Section` headings and the writer did not write them back. Completing
---    a task now flips one `[ ]` to `[x]` on one line and leaves the rest of
---    the document byte-for-byte alone.
---
--- 2. **Task ids are positions in the whole file, never in a filtered view.**
---    `tasks_list{show_completed = false}` used to renumber its results, so an
---    id read off that list and passed to `tasks_complete` completed a
---    different task.
---
--- Paths resolve against the kiln, not the process working directory — the
--- daemon's cwd is wherever it was spawned, so a bare `TASKS.md` meant a
--- different file for every user.

local M = {}

--- Defaults mirror `plugin.yaml`'s config block, which nothing parses;
--- `setup` below is what applies the user's `[plugins.todo-list]` section.
local config = {
    default_file = "TASKS.md",
    show_completed = false,
}

--- Called by the daemon at load with the `[plugins.todo-list]` table.
function M.setup(cfg)
    if type(cfg) ~= "table" then
        return
    end
    if cfg.default_file ~= nil then
        config.default_file = cfg.default_file
    end
    if cfg.show_completed ~= nil then
        config.show_completed = cfg.show_completed
    end
end

--- Absolute path to the tasks file.
---
--- The root a relative `file` hangs off: the ACTIVE kiln (by name, through
--- `cru.kiln.path` — the one name-to-path API), else the workspace, else nil.
local function resolve_root(): string?
    local active = cru.kiln and cru.kiln.active
    if type(active) == "string" then
        local ok, root = pcall(cru.kiln.path, active)
        if ok and type(root) == "string" and root ~= "" then
            return root
        end
    end
    local ok, root = pcall(cru.paths.workspace)
    if ok and type(root) == "string" and root ~= "" then
        return root
    end
    return nil
end

--- Kiln-root resolution used to be dead in production (the daemon registers
--- no kiln path on `cru.paths`), so an existing tasks file lives wherever the
--- daemon's cwd happened to be. Now that the kiln arm is live, reading only
--- the new location would make that list silently look empty — so say where
--- the file used to resolve, once, and move NOTHING: never relocate user
--- data on their behalf.
local warned_legacy = false
local function warn_if_legacy_file(resolved: string, legacy: string): ()
    if warned_legacy then return end
    local ok, seen = pcall(cru.fs.exists, resolved)
    if ok and seen then return end
    ok, seen = pcall(cru.fs.exists, legacy)
    if not ok or not seen then return end
    warned_legacy = true
    cru.log("warn", string.format(
        "todo-list: tasks now resolve to '%s', but a file exists at the old "
            .. "cwd-relative location '%s'. Nothing was moved — move the old "
            .. "file there if it is the one you want.",
        resolved, legacy))
end

--- An absolute `file` is taken as given. A relative one hangs off the active
--- kiln's root, falling back to the workspace and then to the path as written.
local function tasks_file(args: { [string]: any }?): string
    local name = (args and args.file) or config.default_file
    if name:sub(1, 1) == "/" then
        return name
    end
    local root = resolve_root()
    if root then
        -- `name` is known relative here, so string concat is the join.
        local path = root .. "/" .. name
        warn_if_legacy_file(path, name)
        return path
    end
    return name
end

--- Every line of the file, or nil if it does not exist.
local function read_lines(path: string): { string }?
    -- A missing file and an unreadable one land the same way: `io.open`
    -- answers nil, and the caller treats the list as absent.
    local handle = io.open(path, "r")
    if not handle then
        return nil
    end
    local content = handle:read("a")
    handle:close()
    if type(content) ~= "string" then
        return nil
    end
    local lines = {}
    -- Trailing empty field from a final newline is dropped by the pattern.
    for line in (content .. "\n"):gmatch("([^\n]*)\n") do
        lines[#lines + 1] = line
    end
    -- A file ending in "\n" yields one spurious empty line; drop it so a
    -- round trip through write_lines is byte-identical.
    if #lines > 0 and lines[#lines] == "" and content:sub(-1) == "\n" then
        table.remove(lines)
    end
    return lines
end

local function write_lines(path: string, lines: { string }): (boolean, any)
    local handle, open_err = io.open(path, "w")
    if not handle then
        return false, open_err
    end
    -- `pcall`, not a pair: Crucible's `file:write` RAISES on failure and
    -- answers with the handle otherwise, so `write_err` was nil forever and a
    -- full disk escaped this function as a raise.
    local wrote, write_err = pcall(function()
        handle:write(table.concat(lines, "\n") .. "\n")
        return nil
    end)
    handle:close()
    if not wrote then
        return false, write_err
    end
    return true
end

--- Parse tasks out of `lines`, in document order.
---
--- Each task carries `line`, its 1-based index into `lines`, which is what
--- makes a line-local edit possible. `id` is its position among tasks and is
--- stable regardless of any filtering a caller applies afterwards.
--- One task, as every handler below reads it.
export type Task = {
    id: number,
    text: string,
    completed: boolean,
    section: string,
    line: number,
}

local function parse_tasks(lines: { string }): { Task }
    local tasks: { Task } = {}
    local section = "Tasks"
    for index, line in ipairs(lines) do
        local heading = line:match("^#+%s+(.+)$")
        if heading then
            section = heading
        end
        local status, text = line:match("^%s*[-*]%s+%[([xX%s])%]%s*(.*)$")
        if status then
            tasks[#tasks + 1] = {
                id = #tasks + 1,
                -- The `if status` guard proves the pattern matched, so both
                -- captures are strings; `match` types them optional because a
                -- failed match answers with nil for every capture at once.
                text = text :: string,
                completed = status:lower() == "x",
                section = section,
                line = index,
            }
        end
    end
    return tasks
end

local function load(args: { [string]: any }?): (string, { string }?, { Task })
    local path = tasks_file(args)
    local lines = read_lines(path)
    return path, lines, parse_tasks(lines or {})
end

--- What a tool handler answers with: either `{ error = "..." }` or the
--- handler's own result shape. Both cross to JSON, and the daemon reads
--- `error` first.
type ToolResult = { [string]: any }

--- List tasks. `show_completed` defaults to the configured value.
function M.tasks_list(args: { [string]: any }): ToolResult
    local path, _, tasks = load(args)
    local show_completed = args.show_completed
    if show_completed == nil then
        show_completed = config.show_completed
    end

    local results = {}
    for _, task in ipairs(tasks) do
        if show_completed or not task.completed then
            results[#results + 1] = {
                -- Position in the FILE, not in this filtered result: an id
                -- renumbered per view completes the wrong task.
                id = task.id,
                text = task.text,
                completed = task.completed,
                section = task.section,
            }
        end
    end

    return { file = path, count = #results, total = #tasks, tasks = results }
end

--- Append a task, under `section` if that heading exists.
function M.tasks_add(args: { [string]: any }): ToolResult
    if not args.text or args.text == "" then
        return { error = "Task text is required" }
    end

    local path, loaded, tasks = load(args)
    -- Annotate the LOCAL: `loaded or {...}` widens to `{string} | {string}`
    -- but keeps the optional half unless the target says otherwise.
    local lines: { string } = loaded or { "# Tasks", "" }

    local entry = "- [ ] " .. args.text
    local inserted = false

    if args.section then
        -- After the last task of that section, so additions group rather than
        -- landing immediately under the heading in reverse order.
        local last
        for _, task in ipairs(tasks) do
            if task.section == args.section then
                last = task.line
            end
        end
        if last then
            table.insert(lines, last + 1, entry)
            inserted = true
        end
    end

    if not inserted then
        lines[#lines + 1] = entry
    end

    local ok, err = write_lines(path, lines)
    if not ok then
        return { error = "Cannot write " .. path .. ": " .. tostring(err) }
    end

    return {
        success = true,
        message = "Task added: " .. args.text,
        file = path,
        id = #tasks + 1,
        total = #tasks + 1,
    }
end

--- Flip one task to complete, touching only its line.
function M.tasks_complete(args: { [string]: any }): ToolResult
    if not args.id then
        return { error = "Task ID is required" }
    end

    local id = tonumber(args.id)
    if not id then
        return { error = "Invalid task ID: " .. tostring(args.id) }
    end

    local path, lines, tasks = load(args)
    if not lines then
        return { error = "No tasks file at " .. path }
    end
    if id < 1 or id > #tasks or id ~= math.floor(id) then
        return { error = "Invalid task ID: " .. tostring(args.id) }
    end

    local task = tasks[id]
    if task.completed then
        return { success = false, message = "Task already completed" }
    end

    -- One substitution, anchored to the checkbox, count-limited to 1: the task
    -- text itself may contain "[ ]".
    lines[task.line] = lines[task.line]:gsub("%[%s%]", "[x]", 1)

    local ok, err = write_lines(path, lines)
    if not ok then
        return { error = "Cannot write " .. path .. ": " .. tostring(err) }
    end

    return { success = true, message = "Completed: " .. task.text, id = id }
end

--- The first uncompleted task in document order.
function M.tasks_next(args: { [string]: any }): ToolResult
    local path, _, tasks = load(args)

    local remaining = 0
    for _, task in ipairs(tasks) do
        if not task.completed then
            remaining = remaining + 1
        end
    end

    for _, task in ipairs(tasks) do
        if not task.completed then
            return {
                id = task.id,
                text = task.text,
                section = task.section,
                file = path,
                -- Uncompleted tasks after this one, not "everything after this
                -- index", which counted completed ones too.
                remaining = remaining - 1,
            }
        end
    end

    return { message = "All tasks completed!", file = path, total = #tasks }
end

--- /tasks [list|add|next]
function M.tasks_command(args: { [string]: any }, ctx: any): ()
    local subcommand = args._positional and args._positional[1] or "list"

    if subcommand == "list" then
        local result = M.tasks_list({ show_completed = true })
        local lines = { "Tasks:" }
        for _, task in ipairs(result.tasks) do
            lines[#lines + 1] =
                string.format("  %s %d. %s", task.completed and "✓" or "○", task.id, task.text)
        end
        ctx.display_info(table.concat(lines, "\n"))
    elseif subcommand == "add" then
        local text = table.concat(args._positional or {}, " ", 2)
        if text == "" then
            ctx.display_error("Usage: /tasks add <task description>")
            return
        end
        local result = M.tasks_add({ text = text })
        if result.error then
            ctx.display_error(result.error)
        else
            ctx.display_info(result.message)
        end
    elseif subcommand == "next" then
        local result = M.tasks_next({})
        if result.text then
            ctx.display_info(string.format("Next: %s (ID: %d)", result.text, result.id))
        else
            ctx.display_info(result.message)
        end
    else
        ctx.display_error("Unknown subcommand: " .. subcommand)
    end
end

return {
    setup = M.setup,

    tools = {
        tasks_list = {
            desc = "List tasks with their status and stable ids",
            params = {
                { name = "file", type = "string", desc = "Path to tasks file (default: TASKS.md, relative to the kiln)", optional = true },
                { name = "show_completed", type = "boolean", desc = "Include completed tasks", optional = true },
            },
            fn = M.tasks_list,
        },
        tasks_add = {
            desc = "Add a new task",
            params = {
                { name = "text", type = "string", desc = "Task description" },
                { name = "section", type = "string", desc = "Heading to file it under, if it exists", optional = true },
                { name = "file", type = "string", desc = "Path to tasks file (default: TASKS.md, relative to the kiln)", optional = true },
            },
            fn = M.tasks_add,
        },
        tasks_complete = {
            desc = "Mark a task completed. The id is its position in the file, as returned by tasks_list.",
            params = {
                { name = "id", type = "number", desc = "Task ID (from tasks_list)" },
                { name = "file", type = "string", desc = "Path to tasks file (default: TASKS.md, relative to the kiln)", optional = true },
            },
            fn = M.tasks_complete,
        },
        tasks_next = {
            desc = "Get the next available task to work on",
            params = {
                { name = "file", type = "string", desc = "Path to tasks file (default: TASKS.md, relative to the kiln)", optional = true },
            },
            fn = M.tasks_next,
        },
    },

    commands = {
        tasks = {
            desc = "Show task summary",
            hint = "[list|add|next]",
            fn = M.tasks_command,
        },
    },
}
