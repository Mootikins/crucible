--!strict
--- kanban — a board over a folder of markdown tickets.
---
--- The reference implementation of the plugin contract: **a plugin owns data;
--- each frontend draws it natively.** This file owns the tickets and nothing
--- else. It publishes the board with `cru.plugin.publish`, which pushes a
--- `publication_changed` event to every client, and it accepts one command to
--- move a card. It describes no layout, no colour and no widget.
---
--- The earlier version of this plugin declared an Oil view that both frontends
--- rendered. That was the wrong seam: it made a terminal's vocabulary the
--- cross-frontend contract, so a browser inherited cells, sixteen colours and
--- no way to say "stack when narrow". See
--- `docs/Meta/Analysis/Oil in Documents.md`.
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
    -- A ticket is a markdown file. `tickets()` filters on that when it reads,
    -- and without the same rule here a move would write a status line into
    -- any file in the folder that a caller happened to name.
    if file:sub(-3) ~= ".md" then
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

--- The board, as data. No layout, no colour, no widget.
---
--- One flat list plus the column order. A renderer groups; that is a
--- renderer's job, and a web board that wants a different grouping (by
--- assignee, say) should not need this plugin to change.
function M.board(folder: string?, kiln: string?)
    local all = tickets(folder, kiln)
    local out = {}
    for _, t in all do
        table.insert(out, { file = t.file, title = t.title, status = t.status })
    end
    return {
        columns = COLUMNS,
        tickets = out,
        folder = folder or config.folder,
        kiln = kiln,
        dir = board_dir(folder, kiln),
    }
end

--- Publish the board. Every client is told to re-read.
---
--- Keyed by `kanban:board` rather than `board`: a publication key is a global
--- namespace shared by every plugin, and `board` is a word more than one
--- plugin will want.
function M.republish(folder: string?, kiln: string?)
    -- `pcall` because `cru.plugin.publish` is bound per plugin by the loader,
    -- and a plugin body also runs in the bare VM that `cru plugin check` and
    -- the Lua test runner use. Failing to publish there must not fail the move
    -- that preceded it.
    pcall(function()
        cru.plugin.publish("kanban:board", M.board(folder, kiln))
    end)
end

--- Move a ticket, then republish so every client redraws.
---
--- The command is the whole write surface. A frontend never edits a file; it
--- asks this to, and learns the result by being told the data changed.
type MoveResult = { ok: boolean, error: string? }

function M.move_command(args): MoveResult
    if type(args) ~= "table" then
        return { ok = false, error = "expected a table of arguments" }
    end
    local file, to = args.file, args.to
    if type(file) ~= "string" or type(to) ~= "string" then
        return { ok = false, error = "`file` and `to` are required" }
    end
    local ok = move(args.folder, args.kiln, file, to)
    if not ok then
        return { ok = false, error = "no such ticket, or not a column this board has" }
    end
    M.republish(args.folder, args.kiln)
    return { ok = true }
end

return {
    name = "kanban",
    version = "0.1.0",
    description = "A board over a folder of markdown tickets",
    author = "Crucible Contributors",
    license = "MIT",

    setup = M.setup,

    tools = {
        kanban_board = {
            desc = "Read the ticket board as data",
            params = {
                { name = "folder", type = "string", desc = "Ticket folder", optional = true },
                { name = "kiln", type = "string", desc = "Kiln name", optional = true },
            },
            fn = function(args: any)
                local t = if type(args) == "table" then args else {}
                return M.board(t.folder, t.kiln)
            end,
        },
        kanban_move = {
            desc = "Move a ticket to another column",
            params = {
                { name = "file", type = "string", desc = "Ticket file name" },
                { name = "to", type = "string", desc = "Target column" },
                { name = "folder", type = "string", desc = "Ticket folder", optional = true },
                { name = "kiln", type = "string", desc = "Kiln name", optional = true },
            },
            fn = M.move_command,
        },
    },

    -- Commands, not just tools: `plugin.run_command` is what a frontend can
    -- reach over RPC, and the web board is a frontend. Tools are for agents.
    --
    -- These two are the plugin API's pair of shapes. `kanban` re-derives the
    -- board from the ticket files and publishes it: it changes nothing a user
    -- could lose, so it is a `read` even though something observable moves.
    -- `kanban_move` rewrites a ticket's `status:` line, so it is a `write` and
    -- a UI must not press it for you without asking.
    --
    -- Both keep their `hint`: it is one line of free text for a person, and a
    -- dialog is generated from `params` instead.
    commands = {
        kanban = {
            desc = "Publish the ticket board so clients redraw",
            hint = "[folder]",
            effect = "read",
            params = {
                { name = "folder", type = "string", desc = "Ticket folder", optional = true },
                { name = "kiln", type = "string", desc = "Kiln name", optional = true },
            },
            fn = function(args: any)
                local t = if type(args) == "table" then args else {}
                M.republish(t.folder, t.kiln)
                return { ok = true }
            end,
        },
        kanban_move = {
            desc = "Move a ticket to another column",
            hint = "<file> <column>",
            effect = "write",
            params = {
                { name = "file", type = "string", desc = "Ticket file name" },
                { name = "to", type = "string", desc = "Target column" },
                { name = "folder", type = "string", desc = "Ticket folder", optional = true },
                { name = "kiln", type = "string", desc = "Kiln name", optional = true },
            },
            fn = M.move_command,
        },
    },
}
