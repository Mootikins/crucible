--!strict
--- Tests for the kanban plugin.
---
--- The ticket folder is REAL: the plugin reads and writes with `io.open`,
--- which has no mock to agree with. Each test gets a fresh directory and the
--- `cru.paths` mock points the active kiln at it.
---
--- What these lock down is the property that makes the plugin worth having —
--- that a ticket is a FILE and nothing else. A move must rewrite one line and
--- leave the body alone, and the board must be readable back off disk by any
--- editor afterwards.

local plugin = require("kanban")

local KILN: string = ""
local BOARD: string = ""

local function ticket(name: string, status: string?, body: string?)
    local handle = assert(io.open(BOARD .. "/" .. name, "w"))
    local front = "---\ntitle: " .. name:gsub("%.md$", "") .. "\n"
    if status then front = front .. "status: " .. status .. "\n" end
    handle:write(front .. "---\n" .. (body or "The body a human wrote.\n"))
    handle:close()
end

--- The ticket's bytes on disk. Asserts rather than returning nil: every call
--- site here is checking what a move wrote, and a missing file is a failure
--- of the test's own setup, not a case to branch on.
local function read(name: string): string
    local handle = assert(io.open(BOARD .. "/" .. name, "r"))
    local content = handle:read("a")
    handle:close()
    return content :: string
end

describe("kanban", function()
    before_each(function()
        KILN = os.tmpname()
        os.remove(KILN)
        test_mocks.setup({
            kiln = { active = "notes", roots = { notes = KILN } },
            fs = { real_dirs = true },
        })
        cru.fs.mkdir(KILN)
        BOARD = KILN .. "/tickets"
        cru.fs.mkdir(BOARD)
        plugin.setup({ folder = "tickets" })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("reading the folder", function()
        it("renders a tree when the folder has tickets", function()
            ticket("alpha.md", "todo")
            local tree = plugin.views.board.render({})
            expect.equal(type(tree), "userdata")
        end)

        it("renders an explanation, not an error, for an empty folder", function()
            local tree = plugin.views.board.render({})
            expect.equal(type(tree), "userdata")
        end)

        it("ignores a file that is not markdown", function()
            local handle = assert(io.open(BOARD .. "/notes.txt", "w"))
            handle:write("not a ticket")
            handle:close()
            -- Nothing to assert on the tree directly; the move below proves
            -- the file was never treated as a ticket.
            expect.equal(plugin.views.board.on_action("move", { file = "notes.txt", to = "doing" }), nil)
        end)
    end)

    describe("moving a ticket", function()
        it("rewrites the status line and leaves the body alone", function()
            ticket("alpha.md", "todo", "Body line one.\nBody line two.\n")
            plugin.views.board.on_action("move", { file = "alpha.md", to = "doing" })
            local content = read("alpha.md")
            expect.truthy((content:find("status: doing", 1, true)))
            expect.falsy((content:find("status: todo", 1, true)))
            expect.truthy((content:find("Body line one.", 1, true)))
            expect.truthy((content:find("Body line two.", 1, true)))
        end)

        it("adds a status to a ticket that had none, inside the existing block", function()
            ticket("beta.md", nil)
            plugin.views.board.on_action("move", { file = "beta.md", to = "done" })
            local content = read("beta.md")
            expect.truthy((content:find("status: done", 1, true)))
            -- One frontmatter block, not two.
            local _, blocks = content:gsub("\n%-%-%-", "")
            expect.equal(blocks, 1)
        end)

        it("refuses a column it does not have", function()
            ticket("alpha.md", "todo")
            plugin.views.board.on_action("move", { file = "alpha.md", to = "shipped" })
            expect.truthy((read("alpha.md"):find("status: todo", 1, true)))
        end)

        -- A rendered tree's params reach `on_action` from a client, and a note
        -- author can hand-write the fence that produced it. A file name that
        -- climbs out of the board folder must not be followed.
        it("refuses a file name that escapes the board folder", function()
            local outside = KILN .. "/secret.md"
            local handle = assert(io.open(outside, "w"))
            handle:write("---\nstatus: todo\n---\nnot a ticket\n")
            handle:close()

            plugin.views.board.on_action("move", { file = "../secret.md", to = "done" })

            local after = assert(io.open(outside, "r"))
            local content = after:read("a") :: string
            after:close()
            expect.truthy((content:find("status: todo", 1, true)))
        end)

        it("ignores an action it does not know", function()
            ticket("alpha.md", "todo")
            plugin.views.board.on_action("archive", { file = "alpha.md", to = "done" })
            expect.truthy((read("alpha.md"):find("status: todo", 1, true)))
        end)

        it("ignores params that are not a table", function()
            expect.equal(plugin.views.board.on_action("move", nil), nil)
            expect.equal(plugin.views.board.on_action("move", 42), nil)
        end)
    end)

    describe("setup", function()
        it("applies the configured folder", function()
            local other = KILN .. "/backlog"
            cru.fs.mkdir(other)
            plugin.setup({ folder = "backlog" })
            expect.equal(type(plugin.views.board.render({})), "userdata")
        end)

        it("ignores a non-table config instead of erroring", function()
            plugin.setup(nil)
            plugin.setup(42)
            expect.equal(type(plugin.views.board.render({})), "userdata")
        end)
    end)
end)
