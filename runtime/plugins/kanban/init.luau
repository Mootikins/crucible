--!strict
--- kanban — a board over a folder of markdown tickets.
---
--- The point of this plugin is not the board. It is the question the board
--- asks: can a plugin declare a whole VIEW the way `cru.plugin.options` lets
--- it declare a settings form, and have both frontends draw it?
---
--- Everything a ticket is lives in the file. A ticket is a markdown note with
--- a `status:` in its frontmatter, and moving a card rewrites that one line —
--- so the board holds no state of its own, and the folder is still a folder of
--- notes that any editor opens. That is the constraint that makes this worth
--- building rather than storing cards in a database and rendering them.

local M = {}

local COLUMNS = { "todo", "doing", "done" }

local config = {
    folder = "tickets",
}

function M.setup(cfg)
    if type(cfg) ~= "table" then return end
    if type(cfg.folder) == "string" then config.folder = cfg.folder end
end

--- The kiln root a relative folder hangs off, else the workspace.
---
--- `named` comes from the view's own params, and it is FIRST for a reason this
--- plugin exists to expose: a view is rendered without a session, so
--- `cru.kiln.active` — which every other kiln-reading plugin leans on — is
--- empty here. `plugin.view_render` carries a plugin, a view and params, and
--- nothing that says which kiln is in front of the user. Until it does, a
--- board embedded in a note must name its own kiln.
local function root(named: string?): string?
    if type(named) == "string" and named ~= "" then
        local ok, path = pcall(cru.kiln.path, named)
        if ok and type(path) == "string" and path ~= "" then return path end
    end
    local active = cru.kiln and cru.kiln.active
    if type(active) == "string" then
        local ok, path = pcall(cru.kiln.path, active)
        if ok and type(path) == "string" and path ~= "" then return path end
    end
    local ok, path = pcall(cru.paths.workspace)
    if ok and type(path) == "string" and path ~= "" then return path end
    return nil
end

local function board_dir(folder: string?, kiln: string?): string?
    local rel = folder or config.folder
    -- An absolute folder needs no root, and is checked BEFORE resolving one:
    -- otherwise a board with a full path is unreachable on a daemon that has
    -- no active kiln, which is every daemon rendering a view.
    if rel:sub(1, 1) == "/" then return rel end
    local base = root(kiln)
    if not base then return nil end
    return base .. "/" .. rel
end

local function read_file(path: string): string?
    local handle = io.open(path, "r")
    if not handle then return nil end
    local content = handle:read("a")
    handle:close()
    return content
end

local function write_file(path: string, content: string): boolean
    local handle = io.open(path, "w")
    if not handle then return false end
    local ok = pcall(function() handle:write(content) end)
    handle:close()
    return ok
end

--- One ticket, read from the file and nothing else.
type Ticket = {
    file: string,
    title: string,
    status: string,
}

--- Read `status:` and `title:` out of a leading `---` frontmatter block.
---
--- A deliberate hand parse rather than a YAML one: the board reads two scalar
--- keys, and `move` below rewrites the status LINE rather than re-emitting the
--- document. Re-emitting is what made the old todo-list writer delete every
--- heading in the file, and a ticket's body is the part a human wrote.
local function parse(content: string): (string?, string?)
    if content:sub(1, 4) ~= "---\n" then return nil, nil end
    local close = content:find("\n---", 4, true)
    if not close then return nil, nil end
    local block = content:sub(5, close)
    local status = block:match("\nstatus:%s*([%w_-]+)") or block:match("^status:%s*([%w_-]+)")
    local title = block:match('\ntitle:%s*"?([^"\n]+)"?') or block:match('^title:%s*"?([^"\n]+)"?')
    if title then title = (title:gsub("%s+$", "")) end
    return status, title
end

local function tickets(folder: string?, kiln: string?): { Ticket }
    local dir = board_dir(folder, kiln)
    local out: { Ticket } = {}
    if not dir then return out end
    local ok, entries = pcall(cru.fs.list, dir)
    if not ok or type(entries) ~= "table" then return out end

    for _, name in entries do
        if type(name) == "string" and name:sub(-3) == ".md" then
            local path = dir .. "/" .. name
            local content = read_file(path)
            if content then
                local status, title = parse(content)
                table.insert(out, {
                    file = name,
                    -- A ticket with no `title:` is named by its file. Falling
                    -- back to the filename beats hiding the card: an untitled
                    -- ticket is a real ticket someone forgot to title.
                    title = title or (name:gsub("%.md$", "")),
                    status = status or COLUMNS[1],
                })
            end
        end
    end
    table.sort(out, function(a, b) return a.file < b.file end)
    return out
end

--- Rewrite one ticket's status line in place.
local function move(folder: string?, kiln: string?, file: string, to: string): boolean
    -- A file name, never a path: a `params` table reaches here from a rendered
    -- tree, which a note author can hand-write, so `../` must not escape the
    -- board folder.
    if file:find("/", 1, true) or file:find("\\", 1, true) or file:find("..", 1, true) then
        return false
    end
    if not table.find(COLUMNS, to) then return false end

    local dir = board_dir(folder, kiln)
    if not dir then return false end
    local path = dir .. "/" .. file
    local content = read_file(path)
    if not content then return false end

    local replaced, count = content:gsub("(\nstatus:%s*)[%w_-]+", "%1" .. to, 1)
    if count == 0 then
        -- No status line yet: add one inside the existing frontmatter rather
        -- than creating a second block.
        if content:sub(1, 4) == "---\n" then
            replaced = content:gsub("^(---\n)", "%1status: " .. to .. "\n", 1)
        else
            replaced = "---\nstatus: " .. to .. "\n---\n" .. content
        end
    end
    return write_file(path, replaced)
end

local COLUMN_COLOR = {
    todo = "yellow",
    doing = "cyan",
    done = "green",
}

--- One card. The whole card is the click target, and the action names the
--- NEXT column rather than a direction, so the tree carries no ordering the
--- renderer would have to know.
local function card(t: Ticket, folder: string?, kiln: string?): any
    local oil = cru.oil
    local idx = table.find(COLUMNS, t.status) or 1
    local next_col = COLUMNS[(idx % #COLUMNS) + 1]
    local params: { [string]: string } = { file = t.file, to = next_col }
    if folder then params.folder = folder end
    if kiln then params.kiln = kiln end

    return oil.action(
        "move",
        params,
        oil.row({ gap = 1, padding = 1, border = "rounded" },
            oil.text(t.title),
            oil.spacer(),
            oil.text("→ " .. next_col, { dim = true })
        )
    )
end

local function column(name: string, items: { Ticket }, folder: string?, kiln: string?): any
    local oil = cru.oil
    local children = {
        oil.text(string.upper(name) .. "  (" .. #items .. ")", {
            bold = true,
            fg = COLUMN_COLOR[name],
        }),
    }
    for _, t in items do
        table.insert(children, card(t, folder, kiln))
    end
    if #items == 0 then
        table.insert(children, oil.text("nothing here", { dim = true, italic = true }))
    end
    return oil.col({ gap = 1, padding = 1 }, table.unpack(children))
end

--- The board tree. Exported on `M` as well as registered, so a test can call
--- it without a daemon binding underneath.
function M.render(params)
            local oil = cru.oil
            local folder = if type(params) == "table" and type(params.folder) == "string"
                then params.folder
                else nil
            local kiln = if type(params) == "table" and type(params.kiln) == "string"
                then params.kiln
                else nil

            local all = tickets(folder, kiln)
            local columns = {}
            for _, name in COLUMNS do
                local items: { Ticket } = {}
                for _, t in all do
                    if t.status == name then table.insert(items, t) end
                end
                table.insert(columns, column(name, items, folder, kiln))
            end

            if #all == 0 then
                local dir = board_dir(folder, kiln) or "(no kiln named; add \"kiln\" to the fence)"
                return oil.col({ gap = 1, padding = 1, border = "rounded" },
                    oil.text("No tickets found", { bold = true }),
                    oil.text(dir, { dim = true }),
                    oil.text("A ticket is a .md file with `status: todo` in its frontmatter.", {
                        dim = true,
                    })
                )
            end

            return oil.row({ gap = 2 }, table.unpack(columns))
end

function M.on_action(action, params)
    if action ~= "move" then return end
    if type(params) ~= "table" then return end
    local file = params.file
    local to = params.to
    if type(file) ~= "string" or type(to) ~= "string" then return end
    move(params.folder, params.kiln, file, to)
end

-- `pcall` because a plugin body also runs in the test runner's bare VM, which
-- has no per-plugin `cru.plugin` binding. Failing to register there would stop
-- the module from loading at all, and the tests below exercise `M.render`
-- directly — the same functions the daemon registers.
pcall(function()
    cru.plugin.views({
        board = { render = M.render, on_action = M.on_action },
    })
end)

return {
    name = "kanban",
    version = "0.1.0",
    description = "A board over a folder of markdown tickets",
    capabilities = { "filesystem", "kiln", "config" },

    setup = M.setup,

    -- The same two functions `cru.plugin.views` is given above. Declared here
    -- as well so the shape is visible in the spec a reader opens first, and so
    -- a test can call them without a daemon binding underneath — the way
    -- todo-list's tests reach its tools through `spec.tools`.
    views = {
        board = { render = M.render, on_action = M.on_action },
    },
}
