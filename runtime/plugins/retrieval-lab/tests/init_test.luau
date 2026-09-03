--!strict
-- The runner VM has no `cru.on` (the daemon registers it), so the stub
-- records each registration. A test fires the handler it recorded. The kiln
-- mock has no `blocks`, so the stub answers from a fixture table keyed by
-- kiln and path; `cru.vec` is the real module.
local registered: { [string]: any } = {}
;(cru :: any).on = function(event: string, opts: any, fn: any)
    registered[event] = if type(opts) == "function" then opts else fn
end

local fixtures: { [string]: any } = {}
;(cru.kiln :: any).blocks = function(kiln: string, path: string)
    return fixtures[kiln .. "/" .. path] or {}
end

local logged: { string } = {}
;(cru :: any).log = function(level: string, message: string)
    logged[#logged + 1] = message
end

-- The named graph reads and the embedder, as the merged API declares them:
-- `cru.kiln.note(kiln, path)`, `cru.kiln.notes(kiln, limit?)`,
-- `cru.kiln.links(kiln, path)` and `cru.embed(kiln, text)`. Each stub
-- answers from a table a test fills, and `embedded` records every text
-- the embedder saw.
local notes: { [string]: any } = {}
local links: { [string]: any } = {}
local notes_calls = 0
;(cru.kiln :: any).note = function(kiln: string, path: string)
    return notes[kiln .. "/" .. path]
end
;(cru.kiln :: any).notes = function(kiln: string, limit: number?)
    notes_calls += 1
    local out = {}
    for key, note in pairs(notes) do
        if key:sub(1, #kiln + 1) == kiln .. "/" then
            out[#out + 1] = note
        end
    end
    table.sort(out, function(a, b) return a.path < b.path end)
    return out
end
;(cru.kiln :: any).links = function(kiln: string, path: string)
    return links[kiln .. "/" .. path] or { outlinks = {}, backlinks = {} }
end
local embedded: { string } = {}
;(cru :: any).embed = function(kiln: string, text: string)
    embedded[#embedded + 1] = text
    return { #text, 0, 0 }
end

local function note(path: string, title: string, description: string?)
    return {
        path = path,
        title = title,
        content_hash = "",
        tags = {},
        links_to = {},
        properties = if description then { description = description } else {},
        updated_at = "",
        has_embedding = true,
    }
end

local function unit(v: { number }): { number }
    return cru.vec.normalize(v)
end

-- Three directions in the plane: A, the diagonal D at 45 degrees, and B at
-- 90 degrees. cos(A, D) = 0.707 passes the pair gate; cos(A, B) = 0 does not.
local A = unit({ 1, 0, 0 })
local D = unit({ 1, 1, 0 })
local B = unit({ 0, 1, 0 })

local function block(span_start: number, kind: string, vector: { number }?)
    return { span_start = span_start, span_end = span_start + 8, kind = kind, vector = vector }
end

local function hit(index: number, path: string, span_start: number, score: number)
    return {
        index = index,
        kiln = "notes",
        path = path,
        span_start = span_start,
        span_end = span_start + 8,
        kind = "paragraph",
        score = score,
    }
end

describe("retrieval-lab", function()
    local plugin = require("retrieval-lab")
    local config = require("config")

    local function use(strategy: string)
        config.reset()
        plugin.setup({ enabled = true, strategy = strategy })
        plugin.reset_counters()
        fixtures = {}
        logged = {}
        notes = {}
        links = {}
        embedded = {}
        notes_calls = 0
        plugin.forget_graphs()
    end

    describe("arc_post", function()
        it("promotes a hit whose query sits between it and its neighbour, and cites both spans", function()
            use("arc_post")
            fixtures["notes/n.md"] = { block(0, "paragraph", A), block(10, "paragraph", D) }
            -- q lies on the arc from A to D, so the arc reaches cosine 1.
            local q = unit({ 1, 0.5, 0 })
            local own = hit(1, "n.md", 0, cru.vec.cosine(q, A))
            local other = hit(2, "m.md", 0, 0.95)
            fixtures["notes/m.md"] = { block(0, "paragraph", unit({ 1, 0.4, 0.3 })) }

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { other, own } })

            expect.equal(1, out[1].index)
            expect.truthy(out[1].score > 0.999)
            expect.deep_equal({ { 0, 8 }, { 10, 18 } }, out[1].cited)
            expect.equal(2, out[2].index)
            expect.is_nil(out[2].cited)
        end)

        it("refuses an orthogonal pair", function()
            use("arc_post")
            fixtures["notes/n.md"] = { block(0, "paragraph", A), block(10, "paragraph", B) }
            local q = unit({ 1, 1, 0 })
            local own = hit(1, "n.md", 0, cru.vec.cosine(q, A))

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.equal(cru.vec.cosine(q, A), out[1].score)
            expect.is_nil(out[1].cited)
        end)

        it("steps over an excluded kind to the next prose block", function()
            use("arc_post")
            plugin.setup({ exclude_kinds = { "heading", "transition" } })
            fixtures["notes/n.md"] = {
                block(0, "paragraph", A),
                block(10, "heading", D),
                block(20, "paragraph", D),
            }
            local q = unit({ 1, 0.5, 0 })
            local own = hit(1, "n.md", 0, cru.vec.cosine(q, A))

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.deep_equal({ { 0, 8 }, { 20, 28 } }, out[1].cited)
        end)

        it("refuses a neighbour that is only a better point", function()
            use("arc_post")
            fixtures["notes/n.md"] = { block(0, "paragraph", A), block(10, "paragraph", D) }
            -- q lies past D, so the arc's best point is the endpoint D.
            local q = unit({ 1, 2, 0 })
            local own = hit(1, "n.md", 0, cru.vec.cosine(q, A))

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.equal(cru.vec.cosine(q, A), out[1].score)
            expect.is_nil(out[1].cited)
        end)

        it("leaves a whole-note hit and a nameless kiln alone", function()
            use("arc_post")
            local q = unit({ 1, 0.5, 0 })
            local whole = { index = 1, kiln = "notes", path = "n.md", score = 0.5 }
            local nameless = { index = 2, path = "n.md", span_start = 0, span_end = 8, kind = "paragraph", score = 0.4 }
            local hits: { any } = { whole, nameless }

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = {}, limit = 10, hits = hits })

            expect.equal(0.5, out[1].score)
            expect.equal(0.4, out[2].score)
        end)
    end)

    describe("arc_pre", function()
        it("emits one transition row per gated pair, keyed one byte into the first block", function()
            use("arc_pre")
            local event = {
                kiln = "notes",
                path = "n.md",
                blocks = {
                    block(0, "heading", nil),
                    block(10, "paragraph", A),
                    block(20, "paragraph", D),
                    block(30, "paragraph", unit({ -1, 0, 0 })),
                },
            }

            local out = registered["index:blocks"](nil, event)

            expect.equal(1, #out.extra)
            local row = out.extra[1]
            expect.equal(11, row.span_start)
            expect.equal(28, row.span_end)
            expect.equal("transition", row.kind)
            local mid = unit(D)
            local expected = cru.vec.normalize({ A[1] + D[1], A[2] + D[2], A[3] + D[3] })
            expect.truthy(cru.vec.cosine(row.vector, expected) > 0.9999)
            expect.truthy(mid ~= nil)
        end)

        it("does nothing at search time", function()
            use("arc_pre")
            expect.is_nil(registered["search:rerank"](nil, { query_vector = A, kilns = {}, limit = 10, hits = {} }))
        end)
    end)

    describe("bezier_post", function()
        it("needs three blocks", function()
            use("bezier_post")
            fixtures["notes/n.md"] = { block(0, "paragraph", A), block(10, "paragraph", D) }
            local q = unit({ 1, 0.3, 0 })
            local own = hit(1, "n.md", 10, cru.vec.cosine(q, D))

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.equal(cru.vec.cosine(q, D), out[1].score)
            expect.is_nil(out[1].cited)
        end)

        it("scores the curve through the window and cites every span in it", function()
            use("bezier_post")
            fixtures["notes/n.md"] = {
                block(0, "paragraph", A),
                block(10, "paragraph", D),
                block(20, "paragraph", B),
            }
            -- q sits between A and D, nearer an interior sample than either.
            local q = unit({ 1, 0.7, 0 })
            local own = hit(1, "n.md", 10, cru.vec.cosine(q, D))

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.truthy(out[1].score > cru.vec.cosine(q, D))
            expect.truthy(out[1].score > cru.vec.cosine(q, A))
            expect.deep_equal({ { 0, 8 }, { 10, 18 }, { 20, 28 } }, out[1].cited)
        end)

        it("refuses a curve whose best sample is an endpoint", function()
            use("bezier_post")
            fixtures["notes/n.md"] = {
                block(0, "paragraph", A),
                block(10, "paragraph", D),
                block(20, "paragraph", B),
            }
            -- q lies past A, so the curve's best sample is A itself.
            local q = unit({ 1, -0.3, 0 })
            local own = hit(1, "n.md", 10, cru.vec.cosine(q, D))

            local out = registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.equal(cru.vec.cosine(q, D), out[1].score)
            expect.is_nil(out[1].cited)
        end)
    end)

    describe("meta_pre", function()
        it("re-embeds every embedded block under the title, the nearest heading and the description", function()
            use("meta_pre")
            local event = {
                kiln = "notes",
                path = "n.md",
                title = "Note N",
                description = "About N",
                blocks = {
                    { span_start = 0, span_end = 8, kind = "paragraph", text = "intro", vector = A :: { number }? },
                    { span_start = 10, span_end = 18, kind = "heading", text = "## Part one", vector = nil },
                    { span_start = 20, span_end = 28, kind = "paragraph", text = "short", vector = nil },
                    { span_start = 30, span_end = 38, kind = "paragraph", text = "body", vector = D },
                },
            }

            local out = registered["index:blocks"](nil, event)

            expect.equal(2, #embedded)
            expect.equal("Note N\nAbout N\n\nintro", embedded[1])
            expect.equal("Note N > Part one\nAbout N\n\nbody", embedded[2])
            expect.equal(2, #out.replace)
            expect.equal(0, out.replace[1].span_start)
            expect.equal(30, out.replace[2].span_start)
            expect.deep_equal({ #embedded[2], 0, 0 }, out.replace[2].vector)
        end)

        it("honours meta_format", function()
            use("meta_pre")
            plugin.setup({ meta_format = "{meta} | {text}" })
            local event = {
                kiln = "notes",
                path = "Help/Guide.md",
                title = "Guide",
                blocks = { { span_start = 0, span_end = 8, kind = "paragraph", text = "body", vector = A } },
            }

            registered["index:blocks"](nil, event)

            expect.deep_equal({ "Guide | body" }, embedded)
        end)

        it("leaves a nameless kiln alone", function()
            use("meta_pre")
            expect.is_nil(registered["index:blocks"](nil, { path = "n.md", blocks = {} }))
            expect.equal(0, #embedded)
        end)
    end)

    describe("ppr_post", function()
        -- n links to t; m is an island. The scan hits n alone.
        local function graph()
            notes["notes/n.md"] = note("n.md", "N")
            notes["notes/t.md"] = note("t.md", "T")
            notes["notes/m.md"] = note("m.md", "M")
            links["notes/n.md"] = { outlinks = { "t.md" }, backlinks = {} }
            fixtures["notes/t.md"] = { block(0, "heading", nil), block(10, "paragraph", B) }
            fixtures["notes/m.md"] = { block(0, "paragraph", B) }
        end

        it("raises the hit note, and introduces the linked note through its first embedded block", function()
            use("ppr_post")
            plugin.setup({ ppr_extra = 1 })
            graph()
            local own = hit(1, "n.md", 0, 0.6)

            local out = registered["search:rerank"](nil, { query_vector = A, kilns = { "notes" }, limit = 10, hits = { own } })

            expect.equal(2, #out)
            expect.equal(1, out[1].index)
            expect.truthy(out[1].score > 0.6)
            expect.is_nil(out[2].index)
            expect.equal("notes", out[2].kiln)
            expect.equal("t.md", out[2].path)
            expect.equal(10, out[2].span_start)
            expect.equal(18, out[2].span_end)
            expect.truthy(out[2].score > 0)
            expect.truthy(out[2].score < out[1].score)
        end)

        it("reads the graph once, and registers a note handler that drops it", function()
            use("ppr_post")
            graph()
            local event = { query_vector = A, kilns = { "notes" }, limit = 10, hits = { hit(1, "n.md", 0, 0.6) } }

            registered["search:rerank"](nil, event)
            registered["search:rerank"](nil, event)
            expect.equal(1, notes_calls)

            registered["note:modified"](nil, { path = "n.md" })
            registered["search:rerank"](nil, event)
            expect.equal(2, notes_calls)
        end)

        it("leaves a search with no named kiln alone", function()
            use("ppr_post")
            expect.is_nil(registered["search:rerank"](nil, { query_vector = A, kilns = {}, limit = 10, hits = {} }))
        end)
    end)

    describe("meta_ppr", function()
        it("shares the meta_pre index and the ppr_post rerank", function()
            use("meta_ppr")
            expect.equal(plugin.strategies.meta_pre.index, plugin.strategies.meta_ppr.index)
            expect.equal(plugin.strategies.ppr_post.rerank, plugin.strategies.meta_ppr.rerank)
        end)
    end)

    describe("points", function()
        it("returns nil at both stages", function()
            use("points")
            expect.is_nil(registered["search:rerank"](nil, { query_vector = A, kilns = {}, limit = 10, hits = {} }))
            expect.is_nil(registered["index:blocks"](nil, { path = "n.md", blocks = {} }))
        end)
    end)

    describe("counters", function()
        it("count every query, and a win only when a cited hit is in the top ten", function()
            use("arc_post")
            fixtures["notes/n.md"] = { block(0, "paragraph", A), block(10, "paragraph", B) }
            local q = unit({ 1, 1, 0 })
            local own = hit(1, "n.md", 0, cru.vec.cosine(q, A))
            registered["search:rerank"](nil, { query_vector = q, kilns = { "notes" }, limit = 10, hits = { own } })
            expect.equal(1, plugin.counters().queries)
            expect.equal(0, plugin.counters().interior_wins)

            fixtures["notes/n.md"] = { block(0, "paragraph", A), block(10, "paragraph", D) }
            local q2 = unit({ 1, 0.5, 0 })
            local low = hit(1, "n.md", 0, cru.vec.cosine(q2, A))
            local high = hit(2, "m.md", 0, 0.95)
            registered["search:rerank"](nil, { query_vector = q2, kilns = { "notes" }, limit = 10, hits = { high, low } })
            expect.equal(2, plugin.counters().queries)
            expect.equal(1, plugin.counters().interior_wins)
            expect.equal(2, (_G :: any).RETRIEVAL_LAB_COUNTERS.queries)
        end)
    end)

    describe("an unknown strategy", function()
        it("logs once and returns nil", function()
            use("unknown")
            local event = { query_vector = A, kilns = {}, limit = 10, hits = {} }
            expect.is_nil(registered["search:rerank"](nil, event))
            expect.is_nil(registered["search:rerank"](nil, event))
            expect.equal(1, #logged)
            expect.truthy((logged[1]:find("unknown", 1, true)))
        end)
    end)
end)
