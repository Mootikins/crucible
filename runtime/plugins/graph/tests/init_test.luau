--!strict
--- Tests for the graph plugin.
---
--- The property under test is the REDUCTION, because the reduction is the only
--- reason this plugin exists. `cru.kiln.neighbors_with_hops(path, depth)`
--- answers every neighbour within `depth`, each with the hop count the walk
--- reached it at, sorted by hop then by path. Turning that into rings is what
--- a browser cannot do without the whole edge list, so every ring assertion
--- below is an assertion about work staying on the daemon.
---
--- The plugin used to call `cru.kiln.neighbors(path, hop)` once per depth, and
--- each call re-read the whole note list and the whole link table. The walk
--- always knew the hop count and discarded it. These tests record HOW MANY
--- calls the plugin makes, because one call is the property that changed.

local plugin = require("graph")

--- Depths the plugin asked the store for, in order. One entry now.
local ASKED: { number } = {}

--- Stand in for `cru.kiln.neighbors_with_hops` with the daemon's semantics:
--- `rings[hop]` is exactly `hop` steps away, and a call at depth `d` returns
--- rows for rings 1..d, sorted by hop then by path.
local function stub_rings(rings: { { string } })
    ASKED = {}
    cru.kiln.neighbors_with_hops = function(
        _path: string,
        depth: number?
    ): { { path: string, hops: number } }
        local d = depth or 1
        table.insert(ASKED, d)
        local out: { { path: string, hops: number } } = {}
        for hop = 1, d do
            local ring = rings[hop]
            if ring then
                local sorted = table.clone(ring)
                table.sort(sorted)
                for _, path in sorted do
                    table.insert(out, { path = path, hops = hop })
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

        -- The change this test exists for. Three hops used to cost three
        -- calls, and each one re-read the whole note list and the whole link
        -- table. The hop count comes back with the rows now.
        it("asks the store exactly once, at the full depth", function()
            stub_rings({ { "b.md" }, { "c.md" }, { "d.md" } })
            plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 3 })

            expect.equal(#ASKED, 1)
            expect.equal(ASKED[1], 3)
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
            -- One call, and the clamped depth is what it asks for. The clamp
            -- still matters: depth bounds the walk the daemon runs.
            expect.equal(#ASKED, 1)
            expect.equal(ASKED[1], 4)
        end)

        -- The old walk stopped asking when a hop added nothing, because each
        -- further hop cost a full scan. One call makes that moot: an
        -- exhausted component simply returns no rows past its last ring.
        it("returns only the rings the graph actually has", function()
            stub_rings({ { "b.md" } })
            local answer = plugin.commands.graph_neighborhood.fn({ path = "a.md", depth = 4 })

            expect.equal(#ASKED, 1)
            expect.equal(#answer.rings, 1)
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
            expect.equal(#ASKED, 1)
            expect.equal(ASKED[1], 3)
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
            cru.kiln.neighbors_with_hops = function(
                _path: string,
                _depth: number?
            ): { { path: string, hops: number } }
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
