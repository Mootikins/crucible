--!strict
--- Tests for the graph plugin.
---
--- The property under test is the REDUCTION, because the reduction is the only
--- reason this plugin exists. `cru.kiln.neighbors(path, depth)` answers
--- "everything within depth hops" — cumulative, and with no distance attached.
--- Turning that into rings is what a browser cannot do without the whole edge
--- list, so every ring assertion below is an assertion about work staying on
--- the daemon.
---
--- The bundled `cru.kiln.neighbors` mock keys its answer on the path alone and
--- ignores depth (`crucible-lua/src/prelude/test_mocks.rs`), so it cannot
--- express "further away". These tests install their own stub with the real
--- cumulative semantics, and record what depth each call asked for.

local plugin = require("graph")

--- Depths the plugin asked the store for, in order.
local ASKED: { number } = {}

--- Stand in for `cru.kiln.neighbors` with the semantics the daemon has:
--- `rings[hop]` is exactly `hop` steps away, and a call at depth `d` returns
--- the union of rings 1..d.
local function stub_rings(rings: { { string } })
    ASKED = {}
    cru.kiln.neighbors = function(_path: string, depth: number?): { string }
        local d = depth or 1
        table.insert(ASKED, d)
        local out: { string } = {}
        for hop = 1, d do
            local ring = rings[hop]
            if ring then
                for _, path in ring do
                    table.insert(out, path)
                end
            end
        end
        return out
    end
end

describe("graph", function()
    before_each(function()
        test_mocks.setup({ kiln = { active = "notes", roots = { notes = "/mock/kiln" } } })
        plugin.setup({})
        stub_rings({})
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("the neighbourhood", function()
        it("groups notes by how many hops away they are", function()
            stub_rings({ { "b.md" }, { "c.md", "d.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 2 })

            expect.equal(#answer.rings, 2)
            expect.equal(answer.rings[1].hops, 1)
            expect.equal(#answer.rings[1].paths, 1)
            expect.equal(answer.rings[1].paths[1], "b.md")
            expect.equal(answer.rings[2].hops, 2)
            expect.equal(#answer.rings[2].paths, 2)
            expect.equal(answer.rings[2].paths[1], "c.md")
            expect.equal(answer.total, 3)
        end)

        -- The whole point of differencing the hops. `neighbors` is cumulative,
        -- so without this every note within one hop appears in ring 1 AND in
        -- every ring after it, and "distance" means nothing.
        it("does not repeat a nearer note in a further ring", function()
            stub_rings({ { "b.md" }, { "c.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 2 })

            expect.equal(#answer.rings[2].paths, 1)
            expect.equal(answer.rings[2].paths[1], "c.md")
        end)

        it("asks the store once per hop, with that hop as the depth", function()
            stub_rings({ { "b.md" }, { "c.md" }, { "d.md" } })
            plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 3 })

            expect.equal(#ASKED, 3)
            expect.equal(ASKED[1], 1)
            expect.equal(ASKED[2], 2)
            expect.equal(ASKED[3], 3)
        end)

        -- A self-link, or a cycle, would otherwise put the focused note in its
        -- own neighbourhood and a renderer would draw it twice.
        it("never returns the note it started from", function()
            stub_rings({ { "a.md", "b.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 1 })

            expect.equal(#answer.rings[1].paths, 1)
            expect.equal(answer.rings[1].paths[1], "b.md")
        end)

        it("sorts each ring so the same neighbourhood renders the same way", function()
            stub_rings({ { "zeta.md", "alpha.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md" })

            expect.equal(answer.rings[1].paths[1], "alpha.md")
            expect.equal(answer.rings[1].paths[2], "zeta.md")
        end)
    end)

    describe("the arguments a client moves", function()
        -- Depth arrives from a control the user drags. Each hop is a full scan
        -- of the scoped link table daemon-side, so an unbounded number is an
        -- unbounded amount of work asked for by a slider.
        it("clamps a depth above the maximum", function()
            stub_rings({ { "b.md" }, { "c.md" }, { "d.md" }, { "e.md" }, { "f.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 99 })

            expect.equal(answer.depth, 4)
            expect.equal(#ASKED, 4)
        end)

        -- Each hop is a full scan of the scoped link table daemon-side, so a
        -- walk that has run out of graph must stop asking for more of it.
        it("stops asking once a hop adds nothing", function()
            stub_rings({ { "b.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 4 })

            expect.equal(#ASKED, 2)
            expect.equal(answer.total, 1)
        end)

        it("clamps a depth below one", function()
            stub_rings({ { "b.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 0 })

            expect.equal(answer.depth, 1)
            expect.equal(#ASKED, 1)
        end)

        it("stops at the limit and says the answer was cut short", function()
            stub_rings({ { "b.md", "c.md", "d.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({
                path = "a.md",
                depth = 1,
                limit = 2,
            })

            expect.equal(answer.total, 2)
            expect.truthy(answer.truncated)
        end)

        it("says a full answer was not cut short", function()
            stub_rings({ { "b.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md" })

            expect.falsy(answer.truncated)
        end)

        it("uses the configured default depth when the caller names none", function()
            plugin.setup({ depth = 3 })
            stub_rings({ { "b.md" }, { "c.md" }, { "d.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md" })

            expect.equal(answer.depth, 3)
            expect.equal(#ASKED, 3)
        end)
    end)

    describe("failing", function()
        -- One shape whether the read worked or not: a renderer that ignores
        -- `error` still draws an empty neighbourhood instead of throwing on a
        -- missing field.
        it("reports a missing path as an error, in the same shape", function()
            local answer = plugin.commands.graph_neighborhood.fn({})

            expect.equal(answer.root, "")
            expect.equal(#answer.rings, 0)
            expect.truthy(answer.error)
        end)

        it("ignores arguments that are not a table", function()
            expect.truthy(plugin.commands.graph_neighborhood.fn(nil).error)
            expect.truthy(plugin.commands.graph_neighborhood.fn(42).error)
        end)

        it("reports a store failure rather than raising through the command", function()
            cru.kiln.neighbors = function(_path: string, _depth: number?): { string }
                error("no kiln is open")
            end
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md" })

            expect.truthy(answer.error)
            expect.equal(#answer.rings, 0)
        end)

        it("ignores a config that is not a table", function()
            plugin.setup(nil)
            plugin.setup(42)
            stub_rings({ { "b.md" } })
            expect.equal(type(plugin.commands.graph_neighborhood.fn({ path = "a.md" })), "table")
        end)
    end)

    -- The tool and the command run the same function, and only the command is
    -- reachable from `POST /api/plugins/command`. Declaring the tool alone is
    -- the mistake this plugin exists partly to avoid.
    describe("the surface", function()
        it("offers the read as a command, not only as a tool", function()
            expect.truthy(plugin.commands.graph_neighborhood)
            expect.truthy(plugin.tools.graph_neighborhood)
        end)
    end)
end)
