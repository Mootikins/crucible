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

--- Every ticket path staged so far, as the fs mock wants it.
---
--- The plugin writes real bytes with `io.open` and then LISTS the folder with
--- `cru.fs.list`, which the mock answers from this table rather than from the
--- disk — `real_dirs` only makes `mkdir` real. So a staged ticket has to exist
--- in both places, and `seed` re-runs setup with everything staged so far.
local SEEDED: { [string]: string } = {}

local function seed()
    test_mocks.setup({
        kiln = { active = "notes", roots = { notes = KILN } },
        fs = { real_dirs = true, files = SEEDED },
    })
end

--- Write a ticket's exact bytes. `ticket` always puts `title:` first, so a
--- status line can never open the block through it, and a body can never hold
--- a `status:` line of its own.
local function raw_ticket(name: string, content: string)
    local path = BOARD .. "/" .. name
    local handle = assert(io.open(path, "w"))
    handle:write(content)
    handle:close()
    SEEDED[path] = "staged"
    seed()
end

local function ticket(name: string, status: string?, body: string?)
    local path = BOARD .. "/" .. name
    local handle = assert(io.open(path, "w"))
    local front = "---\ntitle: " .. name:gsub("%.md$", "") .. "\n"
    if status then front = front .. "status: " .. status .. "\n" end
    handle:write(front .. "---\n" .. (body or "The body a human wrote.\n"))
    handle:close()
    SEEDED[path] = "staged"
    seed()
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
        SEEDED = {}
        seed()
        cru.fs.mkdir(KILN)
        BOARD = KILN .. "/tickets"
        cru.fs.mkdir(BOARD)
        plugin.setup({ folder = "tickets" })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("reading the folder", function()
        it("publishes a flat ticket list and the column order", function()
            ticket("alpha.md", "todo")
            local board = plugin.tools.kanban_board.fn({})
            expect.equal(#board.tickets, 1)
            expect.equal(board.tickets[1].file, "alpha.md")
            expect.equal(board.tickets[1].status, "todo")
            -- The column ORDER is data. Grouping is the renderer's job, so a
            -- board wanting a different grouping needs no change here.
            expect.equal(board.columns[1], "todo")
            expect.equal(#board.columns, 3)
        end)

        it("answers with an empty list, not an error, for an empty folder", function()
            local board = plugin.tools.kanban_board.fn({})
            expect.equal(#board.tickets, 0)
            expect.equal(#board.columns, 3)
        end)

        it("names a ticket by its file when it has no title", function()
            local path = BOARD .. "/untitled.md"
            local handle = assert(io.open(path, "w"))
            handle:write("---\nstatus: todo\n---\nbody\n")
            handle:close()
            SEEDED[path] = "staged"
            seed()
            local board = plugin.tools.kanban_board.fn({})
            expect.equal(board.tickets[1].title, "untitled")
        end)

        it("ignores a file that is not markdown", function()
            local path = BOARD .. "/notes.txt"
            local handle = assert(io.open(path, "w"))
            handle:write("not a ticket")
            handle:close()
            SEEDED[path] = "staged"
            seed()
            -- Nothing to assert on the tree directly; the move below proves
            -- the file was never treated as a ticket.
            expect.falsy(plugin.tools.kanban_move.fn({ file = "notes.txt", to = "doing" }).ok)
        end)
    end)

    describe("moving a ticket", function()
        it("rewrites the status line and leaves the body alone", function()
            ticket("alpha.md", "todo", "Body line one.\nBody line two.\n")
            plugin.tools.kanban_move.fn({ file = "alpha.md", to = "doing" })
            local content = read("alpha.md")
            expect.truthy((content:find("status: doing", 1, true)))
            expect.falsy((content:find("status: todo", 1, true)))
            expect.truthy((content:find("Body line one.", 1, true)))
            expect.truthy((content:find("Body line two.", 1, true)))
        end)

        it("adds a status to a ticket that had none, inside the existing block", function()
            ticket("beta.md", nil)
            plugin.tools.kanban_move.fn({ file = "beta.md", to = "done" })
            local content = read("beta.md")
            expect.truthy((content:find("status: done", 1, true)))
            -- One frontmatter block, not two.
            local _, blocks = content:gsub("\n%-%-%-", "")
            expect.equal(blocks, 1)
        end)

        it("rewrites the frontmatter status, not a status line in the body", function()
            -- `parse` reads `status:` inside the frontmatter block only, so a
            -- ticket with no status there is a legal ticket: it shows in the
            -- default column. The move ran its `gsub` over the WHOLE document
            -- and rewrote the first `status:` anywhere, which for this ticket
            -- is a line inside a fenced code block a human wrote.
            raw_ticket(
                "doc.md",
                "---\ntitle: doc\n---\n"
                    .. "How to set it:\n\n```yaml\nstatus: todo\n```\n"
            )
            plugin.tools.kanban_move.fn({ file = "doc.md", to = "doing" })
            local content = read("doc.md")
            local head_end = (content:find("\n---", 4, true)) :: number
            local block = content:sub(1, head_end)
            local body = content:sub(head_end + 1)
            expect.truthy((block:find("status: doing", 1, true)))
            expect.truthy((body:find("status: todo", 1, true)))
        end)

        it("rewrites a status line that opens the frontmatter block", function()
            -- The rewrite runs against the block, which starts AFTER the
            -- opening fence, so a status line in the first position has no
            -- leading newline for the main pattern to anchor on. `parse`
            -- accepts that position, so the move must too, or it writes a
            -- second `status:` key into the same block.
            raw_ticket("first.md", "---\nstatus: todo\ntitle: first\n---\nBody.\n")
            plugin.tools.kanban_move.fn({ file = "first.md", to = "done" })
            local content = read("first.md")
            expect.truthy((content:find("status: done", 1, true)))
            expect.falsy((content:find("status: todo", 1, true)))
            local _, keys = content:gsub("status:", "")
            expect.equal(keys, 1)
        end)

        it("replaces a quoted status rather than adding a second key", function()
            -- `[%w_-]+` matches neither `"todo"` nor `to do`, so the rewrite
            -- found nothing and the insert path wrote a SECOND `status:` key
            -- into the same block. A value the reader cannot parse is still a
            -- value a human typed.
            raw_ticket("quoted.md", '---\ntitle: q\nstatus: "todo"\n---\nBody.\n')
            plugin.tools.kanban_move.fn({ file = "quoted.md", to = "doing" })
            local content = read("quoted.md")
            local _, keys = content:gsub("status:", "")
            expect.equal(keys, 1)
            expect.truthy((content:find("status: doing", 1, true)))
        end)

        it("does not prepend a second block to a CRLF ticket", function()
            -- The fence test read `---\n`, which a CRLF file spells `---\r\n`.
            -- The ticket then took a whole second frontmatter block in front
            -- of the one it already had.
            raw_ticket("crlf.md", "---\r\ntitle: c\r\nstatus: todo\r\n---\r\nBody.\r\n")
            plugin.tools.kanban_move.fn({ file = "crlf.md", to = "done" })
            local content = read("crlf.md")
            local _, blocks = content:gsub("%-%-%-", "")
            expect.equal(blocks, 2)
            expect.truthy((content:find("status: done", 1, true)))
        end)

        it("refuses a column it does not have", function()
            ticket("alpha.md", "todo")
            plugin.tools.kanban_move.fn({ file = "alpha.md", to = "shipped" })
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

            plugin.tools.kanban_move.fn({ file = "../secret.md", to = "done" })

            local after = assert(io.open(outside, "r"))
            local content = after:read("a") :: string
            after:close()
            expect.truthy((content:find("status: todo", 1, true)))
        end)


        it("ignores params that are not a table", function()
            expect.falsy(plugin.tools.kanban_move.fn(nil).ok)
            expect.falsy(plugin.tools.kanban_move.fn(42).ok)
        end)
    end)

    describe("setup", function()
        it("applies the configured folder", function()
            local other = KILN .. "/backlog"
            cru.fs.mkdir(other)
            plugin.setup({ folder = "backlog" })
            local board = plugin.tools.kanban_board.fn({})
            expect.equal(board.folder, "backlog")
            expect.equal(board.dir, other)
        end)

        it("ignores a non-table config instead of erroring", function()
            plugin.setup(nil)
            plugin.setup(42)
            expect.equal(type(plugin.tools.kanban_board.fn({})), "table")
        end)
    end)
end)
