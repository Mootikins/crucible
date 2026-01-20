--- Todo List Plugin
--- Manages tasks in TASKS.md format

local M = {}

local function get_tasks_file(args)
    return args.file or "TASKS.md"
end

local function read_tasks(filepath)
    local file = io.open(filepath, "r")
    if not file then
        return {}
    end
    
    local tasks = {}
    local current_section = "Tasks"
    
    for line in file:lines() do
        local header = line:match("^#+%s+(.+)$")
        if header then
            current_section = header
        end
        
        local status, text = line:match("^%s*%-%s+%[([x%s])%]%s+(.+)$")
        if status and text then
            table.insert(tasks, {
                text = text,
                completed = status == "x",
                section = current_section,
                line = #tasks + 1
            })
        end
    end
    
    file:close()
    return tasks
end

local function write_tasks(filepath, tasks)
    local file = io.open(filepath, "w")
    if not file then
        return false, "Cannot open file for writing"
    end
    
    file:write("# Tasks\n\n")
    
    for _, task in ipairs(tasks) do
        local checkbox = task.completed and "[x]" or "[ ]"
        file:write(string.format("- %s %s\n", checkbox, task.text))
    end
    
    file:close()
    return true
end

--- List all tasks from TASKS.md
-- @tool name="tasks_list" desc="List all tasks with their status"
-- @param file string? "Path to tasks file (default: TASKS.md)"
-- @param show_completed boolean? "Include completed tasks"
function M.tasks_list(args)
    local filepath = get_tasks_file(args)
    local tasks = read_tasks(filepath)
    local show_completed = args.show_completed ~= false
    
    local results = {}
    for i, task in ipairs(tasks) do
        if show_completed or not task.completed then
            table.insert(results, {
                id = i,
                text = task.text,
                completed = task.completed,
                section = task.section
            })
        end
    end
    
    return {
        file = filepath,
        count = #results,
        tasks = results
    }
end

--- Add a new task to TASKS.md
-- @tool name="tasks_add" desc="Add a new task"
-- @param text string "Task description"
-- @param file string? "Path to tasks file (default: TASKS.md)"
function M.tasks_add(args)
    if not args.text or args.text == "" then
        return { error = "Task text is required" }
    end
    
    local filepath = get_tasks_file(args)
    local tasks = read_tasks(filepath)
    
    table.insert(tasks, {
        text = args.text,
        completed = false,
        section = "Tasks"
    })
    
    local ok, err = write_tasks(filepath, tasks)
    if not ok then
        return { error = err }
    end
    
    return {
        success = true,
        message = "Task added: " .. args.text,
        total = #tasks
    }
end

--- Mark a task as complete
-- @tool name="tasks_complete" desc="Mark a task as completed"
-- @param id number "Task ID (from tasks_list)"
-- @param file string? "Path to tasks file (default: TASKS.md)"
function M.tasks_complete(args)
    if not args.id then
        return { error = "Task ID is required" }
    end
    
    local filepath = get_tasks_file(args)
    local tasks = read_tasks(filepath)
    
    local id = tonumber(args.id)
    if not id or id < 1 or id > #tasks then
        return { error = "Invalid task ID: " .. tostring(args.id) }
    end
    
    if tasks[id].completed then
        return { 
            success = false, 
            message = "Task already completed" 
        }
    end
    
    tasks[id].completed = true
    
    local ok, err = write_tasks(filepath, tasks)
    if not ok then
        return { error = err }
    end
    
    return {
        success = true,
        message = "Completed: " .. tasks[id].text
    }
end

--- Get the next uncompleted task
-- @tool name="tasks_next" desc="Get the next available task to work on"
-- @param file string? "Path to tasks file (default: TASKS.md)"
function M.tasks_next(args)
    local filepath = get_tasks_file(args)
    local tasks = read_tasks(filepath)
    
    for i, task in ipairs(tasks) do
        if not task.completed then
            return {
                id = i,
                text = task.text,
                section = task.section,
                remaining = #tasks - i
            }
        end
    end
    
    return {
        message = "All tasks completed!",
        total = #tasks
    }
end

--- /tasks command handler
-- @command name="tasks" desc="Show task summary" hint="[list|add|next]"
function M.tasks_command(args, ctx)
    local subcommand = args._positional and args._positional[1] or "list"
    
    if subcommand == "list" then
        local result = M.tasks_list({ show_completed = true })
        local lines = { "Tasks:" }
        for _, task in ipairs(result.tasks) do
            local mark = task.completed and "✓" or "○"
            table.insert(lines, string.format("  %s %d. %s", mark, task.id, task.text))
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

return M
