--- daily-notes — create and navigate dated journal notes.
---
--- Paths resolve against the KILN, never the process working directory. The
--- daemon's cwd is wherever it was spawned (`%h` for the systemd unit, the
--- repo root for a shell-started one), so a relative `Journal/` meant a
--- different destination for every user and dropped a stray `Journal/` next to
--- whatever directory the daemon happened to start in. Same reason the plugin
--- goes through `cru.fs` rather than `io.open` and `os.execute("mkdir -p")`:
--- the shell form interpolated a config value into a command line unescaped.

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
--- journal outside the kiln. A relative one hangs off the kiln root, falling
--- back to the workspace and finally to the cwd, because `paths.kiln()` raises
--- rather than returning nil when no kiln is mounted (see `paths.rs`).
local function notes_dir()
    if config.folder:sub(1, 1) == "/" then
        return config.folder
    end
    for _, accessor in ipairs({ cru.paths.kiln, cru.paths.workspace }) do
        local ok, root = pcall(accessor)
        if ok and root and root ~= "" then
            return cru.paths.join(root, config.folder)
        end
    end
    return config.folder
end

local function date_string(timestamp)
    return os.date(config.date_format, timestamp)
end

local function note_path(timestamp)
    return cru.paths.join(notes_dir(), date_string(timestamp) .. ".md")
end

--- `nil` for a missing or unreadable template, so a bad path degrades to the
--- built-in body instead of failing the write.
local function read_template()
    if config.template == "" then
        return nil
    end
    local ok, content = pcall(cru.fs.read, config.template)
    if not ok then
        return nil
    end
    return content
end

local function default_body(date_str)
    return "# " .. date_str .. "\n\n## Notes\n\n## Tasks\n\n- [ ] \n"
end

local function create_note(timestamp)
    local path = note_path(timestamp)
    local date_str = date_string(timestamp)

    local ok, err = pcall(cru.fs.mkdir, notes_dir())
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

    local wrote, write_err = pcall(cru.fs.write, path, content)
    if not wrote then
        return nil, "Cannot create file: " .. tostring(write_err)
    end
    return path
end

--- Timestamp for an explicit `YYYY-MM-DD`, or now. Second return is an error.
local function parse_date(date)
    if not date then
        return os.time()
    end
    local y, m, d = date:match("^(%d%d%d%d)-(%d%d)-(%d%d)$")
    if not y then
        return nil, "Invalid date format. Use YYYY-MM-DD"
    end
    -- Noon, not midnight: `os.time` interprets the fields as local time, and a
    -- midnight timestamp lands on the previous day in any zone observing DST
    -- that morning, so `daily_create{date="2025-06-15"}` could write
    -- `2025-06-14.md`.
    return os.time({ year = tonumber(y), month = tonumber(m), day = tonumber(d), hour = 12 })
end

local function exists(path)
    local ok, present = pcall(cru.fs.exists, path)
    return ok and present
end

--- Create the note for a date, or report that it already exists.
function M.daily_create(args)
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
function M.daily_open(args)
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
function M.daily_list(args)
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
