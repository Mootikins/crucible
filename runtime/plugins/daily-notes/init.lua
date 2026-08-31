--!strict
--- daily-notes — create and navigate dated journal notes.
---
--- Paths resolve against the KILN, never the process working directory. The
--- daemon's cwd is wherever it was spawned (`%h` for the systemd unit, the
--- repo root for a shell-started one), so a relative `Journal/` meant a
--- different destination for every user and dropped a stray `Journal/` next to
--- whatever directory the daemon happened to start in. Files are read and
--- written with `io.open`; directories go through `cru.fs.mkdir` rather than
--- `os.execute("mkdir -p")`, because the shell form interpolated a config
--- value into a command line unescaped.

local M = {}

--- Defaults mirror `plugin.yaml`'s config block. That block is documentation —
--- `PluginManifest` has no `config` field, so nothing parses it — and `setup`
--- below is what actually applies the user's `[plugins.daily-notes]` section.
--- Keep the two in step.
local config = {
    folder = "Journal",
    template = "",
    date_format = "%Y-%m-%d",
}

--- Called by the daemon at load with the `[plugins.daily-notes]` table.
---
--- Without this the manifest advertised three knobs that did nothing.
function M.setup(cfg)
    if type(cfg) ~= "table" then
        return
    end
    for _, key in ipairs({ "folder", "template", "date_format" }) do
        if cfg[key] ~= nil then
            config[key] = cfg[key]
        end
    end
end

--- Where notes live, as an absolute path.
---
--- An absolute `folder` is taken as given — that is how a user puts their
--- journal outside the kiln. A relative one hangs off the ACTIVE kiln's root
--- (by name, through `cru.kiln.path` — the one name-to-path API), falling
--- back to the workspace and finally to the cwd.
---
--- Kiln-root resolution used to be dead in production (the daemon registers
--- no kiln path on `cru.paths`), so an existing journal lives wherever the
--- daemon's cwd happened to be. Now that the kiln arm is live, reading only
--- the new location would make that journal silently look empty — so say
--- where it used to resolve, once, and move NOTHING: never relocate user
--- data on their behalf.
local warned_legacy = false
local function warn_if_legacy_dir(resolved: string): ()
    if warned_legacy then return end
    local ok, seen = pcall(cru.fs.is_dir, resolved)
    if ok and seen then return end
    ok, seen = pcall(cru.fs.is_dir, config.folder)
    if not ok or not seen then return end
    warned_legacy = true
    cru.log("warn", string.format(
        "daily-notes: notes now resolve to '%s', but a directory exists at "
            .. "the old cwd-relative location '%s'. Nothing was moved — move "
            .. "the old directory there if it is the one you want.",
        resolved, config.folder))
end

--- The root the journal folder hangs off: the active kiln, else the
--- workspace, else nil.
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

local function notes_dir(): string
    if config.folder:sub(1, 1) == "/" then
        return config.folder
    end
    local root = resolve_root()
    if root then
        -- `config.folder` is known relative here, so concat is the join.
        local dir = root .. "/" .. config.folder
        warn_if_legacy_dir(dir)
        return dir
    end
    return config.folder
end

local function date_string(timestamp: number?): string
    -- `os.date` answers a TABLE for a `*t` format, and `date_format` is user
    -- config, so this cannot be cast — it has to be checked. A `*t` here would
    -- otherwise reach `note_path` and concatenate a table into a file name.
    local formatted = os.date(config.date_format, timestamp)
    if type(formatted) ~= "string" then
        error("daily-notes: date_format must produce a string, not a table. "
            .. "Remove the `*t` or `!*t` from [plugins.daily-notes] date_format.")
    end
    return formatted
end

local function note_path(timestamp: number?): string
    return notes_dir() .. "/" .. date_string(timestamp) .. ".md"
end

--- `nil` for a missing or unreadable template, so a bad path degrades to the
--- built-in body instead of failing the write.
local function read_template(): string?
    if config.template == "" then
        return nil
    end
    -- A missing template and an unreadable one land the same way: `io.open`
    -- answers nil, and the built-in body is used.
    local handle = io.open(config.template, "r")
    if not handle then
        return nil
    end
    local content = handle:read("a")
    handle:close()
    return content
end

local function default_body(date_str: string): string
    return "# " .. date_str .. "\n\n## Notes\n\n## Tasks\n\n- [ ] \n"
end

local function create_note(timestamp: number?): (string?, string?)
    local path = note_path(timestamp)
    local date_str = date_string(timestamp)

    -- The inner function returns nil so `pcall` has a second slot for the
    -- error to bind to: `cru.fs.mkdir` answers with nothing, and Luau types
    -- `pcall` as `(boolean, R...)`.
    local ok, err = pcall(function()
        cru.fs.mkdir(notes_dir())
        return nil
    end)
    if not ok then
        return nil, "Cannot create directory: " .. tostring(err)
    end

    local template = read_template()
    local content
    if template then
        content = template:gsub("{{date}}", date_str):gsub("{{title}}", date_str)
    else
        content = default_body(date_str)
    end

    local handle, open_err = io.open(path, "w")
    if not handle then
        return nil, "Cannot create file: " .. tostring(open_err)
    end
    -- `pcall`, not a `(nil, err)` pair: Crucible's `file:write` RAISES on a
    -- failed write and answers with the handle otherwise, so
    -- `local wrote, write_err = handle:write(...)` left `write_err` nil
    -- forever and let a full disk escape this function as a raise instead of
    -- the `(nil, message)` every caller here reads.
    local wrote, write_err = pcall(function()
        handle:write(content)
        return nil
    end)
    handle:close()
    if not wrote then
        return nil, "Cannot create file: " .. tostring(write_err)
    end
    return path, nil
end

--- Timestamp for an explicit `YYYY-MM-DD`, or now. Second return is an error.
local function parse_date(date: string?): (number?, string?)
    if not date then
        return os.time(), nil
    end
    local y, m, d = date:match("^(%d%d%d%d)-(%d%d)-(%d%d)$")
    if not y then
        return nil, "Invalid date format. Use YYYY-MM-DD"
    end
    -- Noon, not midnight: `os.time` interprets the fields as local time, and a
    -- midnight timestamp lands on the previous day in any zone observing DST
    -- that morning, so `daily_create{date="2025-06-15"}` could write
    -- `2025-06-14.md`.
    return os.time({
        year = tonumber(y) :: number,
        month = tonumber(m) :: number,
        day = tonumber(d) :: number,
        hour = 12,
    }), nil
end

local function exists(path: string): boolean
    local ok, present = pcall(cru.fs.exists, path)
    return ok and present
end

--- What a tool handler answers with: either `{ error = "..." }` or the
--- handler's own result shape. Both cross to JSON, and the daemon reads
--- `error` first.
type ToolResult = { [string]: any }

--- Create the note for a date, or report that it already exists.
function M.daily_create(args: { [string]: any }): ToolResult
    local timestamp, err = parse_date(args.date)
    if not timestamp then
        return { error = err }
    end

    local path = note_path(timestamp)
    if exists(path) then
        return { path = path, created = false, message = "Daily note already exists" }
    end

    local created_path, create_err = create_note(timestamp)
    if not created_path then
        return { error = create_err }
    end

    return {
        path = created_path,
        created = true,
        message = "Created daily note: " .. created_path,
    }
end

--- Open a date's note, creating it if missing.
function M.daily_open(args: { [string]: any }): ToolResult
    local timestamp, err = parse_date(args.date)
    if not timestamp then
        return { error = err }
    end

    local path = note_path(timestamp)
    local created = false

    if not exists(path) then
        local _, create_err = create_note(timestamp)
        if create_err then
            return { error = create_err }
        end
        created = true
    end

    return { path = path, created = created, date = date_string(timestamp) }
end

--- The last `days` days, newest first, each flagged with whether it exists.
function M.daily_list(args: { [string]: any }): ToolResult
    local days = args.days or 7
    if type(days) ~= "number" or days < 1 then
        return { error = "days must be a positive number" }
    end

    local notes = {}
    local now = os.time()
    for i = 0, days - 1 do
        local timestamp = now - (i * 86400)
        local path = note_path(timestamp)
        notes[#notes + 1] = {
            date = date_string(timestamp),
            path = path,
            exists = exists(path),
        }
    end

    return { count = #notes, notes = notes }
end

--- /daily [today|yesterday|YYYY-MM-DD|list]
function M.daily_command(args: { [string]: any }, ctx: any): ()
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
            lines[#lines + 1] = string.format("  %s %s", note.exists and "✓" or "○", note.date)
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
        ctx.display_info(string.format("%s: %s", result.created and "Created" or "Opened", result.path))
    end
end

return {
    name = "daily-notes",
    version = "1.1.0",
    description = "Create and manage daily journal notes",
    capabilities = { "filesystem", "kiln", "config" },

    setup = M.setup,

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
