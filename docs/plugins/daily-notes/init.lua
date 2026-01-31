local M = {}

local config = {
    folder = "Journal",
    template = "",
    date_format = "%Y-%m-%d"
}

local function get_date_string(date)
    return os.date(config.date_format, date)
end

local function get_daily_path(date)
    local date_str = get_date_string(date)
    return config.folder .. "/" .. date_str .. ".md"
end

local function file_exists(path)
    local file = io.open(path, "r")
    if file then
        file:close()
        return true
    end
    return false
end

local function read_template()
    if config.template == "" then
        return nil
    end
    local file = io.open(config.template, "r")
    if not file then
        return nil
    end
    local content = file:read("*all")
    file:close()
    return content
end

local function create_daily_note(date)
    local path = get_daily_path(date)
    local date_str = get_date_string(date)

    local dir = config.folder
    os.execute("mkdir -p " .. dir)

    local file = io.open(path, "w")
    if not file then
        return nil, "Cannot create file: " .. path
    end

    local template = read_template()
    if template then
        local content = template:gsub("{{date}}", date_str)
        content = content:gsub("{{title}}", date_str)
        file:write(content)
    else
        file:write("# " .. date_str .. "\n\n")
        file:write("## Notes\n\n")
        file:write("## Tasks\n\n")
        file:write("- [ ] \n")
    end

    file:close()
    return path
end

--- Create today's daily note
function M.daily_create(args)
    local timestamp = os.time()

    if args.date then
        local y, m, d = args.date:match("(%d+)-(%d+)-(%d+)")
        if y and m and d then
            timestamp = os.time({ year = tonumber(y), month = tonumber(m), day = tonumber(d) })
        else
            return { error = "Invalid date format. Use YYYY-MM-DD" }
        end
    end

    local path = get_daily_path(timestamp)

    if file_exists(path) then
        return {
            path = path,
            created = false,
            message = "Daily note already exists"
        }
    end

    local created_path, err = create_daily_note(timestamp)
    if not created_path then
        return { error = err }
    end

    return {
        path = created_path,
        created = true,
        message = "Created daily note: " .. created_path
    }
end

--- Open today's daily note (create if missing)
function M.daily_open(args)
    local timestamp = os.time()

    if args.date then
        local y, m, d = args.date:match("(%d+)-(%d+)-(%d+)")
        if y and m and d then
            timestamp = os.time({ year = tonumber(y), month = tonumber(m), day = tonumber(d) })
        end
    end

    local path = get_daily_path(timestamp)
    local created = false

    if not file_exists(path) then
        local _, err = create_daily_note(timestamp)
        if err then
            return { error = err }
        end
        created = true
    end

    return {
        path = path,
        created = created,
        date = get_date_string(timestamp)
    }
end

--- List recent daily notes
function M.daily_list(args)
    local days = args.days or 7
    local notes = {}
    local now = os.time()

    for i = 0, days - 1 do
        local timestamp = now - (i * 86400)
        local path = get_daily_path(timestamp)
        local exists = file_exists(path)

        table.insert(notes, {
            date = get_date_string(timestamp),
            path = path,
            exists = exists
        })
    end

    return {
        count = #notes,
        notes = notes
    }
end

--- /daily command handler
function M.daily_command(args, ctx)
    local subcommand = args._positional and args._positional[1] or "today"
    local date = nil

    if subcommand == "today" then
        date = nil
    elseif subcommand == "yesterday" then
        date = os.date("%Y-%m-%d", os.time() - 86400)
    elseif subcommand:match("^%d%d%d%d%-%d%d%-%d%d$") then
        date = subcommand
    elseif subcommand == "list" then
        local result = M.daily_list({ days = 7 })
        local lines = { "Recent daily notes:" }
        for _, note in ipairs(result.notes) do
            local mark = note.exists and "✓" or "○"
            table.insert(lines, string.format("  %s %s", mark, note.date))
        end
        ctx.display_info(table.concat(lines, "\n"))
        return
    else
        ctx.display_error("Usage: /daily [today|yesterday|YYYY-MM-DD|list]")
        return
    end

    local result = M.daily_open({ date = date })
    if result.error then
        ctx.display_error(result.error)
    else
        local action = result.created and "Created" or "Opened"
        ctx.display_info(string.format("%s: %s", action, result.path))
    end
end

return {
    name = "daily-notes",
    version = "1.0.0",
    description = "Create and manage daily journal notes",

    tools = {
        daily_create = {
            desc = "Create a daily note for today or a specific date",
            params = {
                { name = "date", type = "string", desc = "Date in YYYY-MM-DD format (default: today)", optional = true },
            },
            fn = M.daily_create,
        },
        daily_open = {
            desc = "Open today's daily note, creating if needed",
            params = {
                { name = "date", type = "string", desc = "Date in YYYY-MM-DD format (default: today)", optional = true },
            },
            fn = M.daily_open,
        },
        daily_list = {
            desc = "List recent daily notes",
            params = {
                { name = "days", type = "number", desc = "Number of days to look back (default: 7)", optional = true },
            },
            fn = M.daily_list,
        },
    },

    commands = {
        daily = {
            desc = "Open or create daily note",
            hint = "[today|yesterday|YYYY-MM-DD]",
            fn = M.daily_command,
        },
    },
}
