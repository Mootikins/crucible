--!strict
--- retrieval-lab — a proof of concept, not a retrieval setting.
---
--- The plugin proves three things: that the `search:rerank` and
--- `index:blocks` stages carry a real strategy, that the `cru.vec` and
--- `cru.kiln.blocks` bindings are enough to compose one, and that the block
--- trajectory idea is measurable. It does not show that a strategy helps.
--- Read WHAT THE LAB MEASURED below before you enable one.
---
--- A strategy is a table with `rerank(event)` and `index(event)`. `rerank`
--- runs at `search:rerank`, after the daemon merges the hits of every kiln
--- and before it cuts the list. `index` runs at `index:blocks`, before the
--- pipeline writes a note's rows. Rust owns the arithmetic (`cru.vec`) and
--- the stored rows (`cru.kiln.blocks`); this file only composes them.
---
--- Off by default. Enable one strategy for a measurement:
---
---   [plugins.retrieval-lab]
---   enabled = true
---   strategy = "arc_post"
---
--- The plugin keeps two counters in the global `RETRIEVAL_LAB_COUNTERS`:
--- `queries`, one per rerank call, and `interior_wins`, one per rerank call
--- that left a cited hit in the top ten. A strategy cites only when the
--- curve's interior beat both endpoints. Nothing here ships as a default.
---
--- WHAT THE LAB MEASURED
---
--- Eight runs scored every strategy against `points` on two corpora, with
--- the fastembed model bge-small-en-v1.5. The corpora are an adversarial
--- fixture of 216 notes and the `docs/Help` plus `docs/Guides` documentary
--- corpus, with 80 golden queries.
---
--- At note level nothing moved. Each strategy changed hit@1 and MRR by
--- about one point on the documentary corpus. The gains and the losses
--- cancelled, query for query.
---
--- The pair and the curve strategies win at block level only. Block-level
--- hit@1 rose from 0.325 under `points` to between 0.500 and 0.575. The
--- passage@10 measure, which asks for both blocks of a two-block answer,
--- reached 0.675. The point baseline scores zero there by construction,
--- because it never names two blocks.
---
--- `meta_pre` and `ppr_post` lost. Both raise a note-level signal, and both
--- fell under `points` at note level and on the multi-hop set. `slate_post`
--- with the ordered plan gained a little at rank ten and lost nothing; with
--- an interleaved plan it lost.
---
--- The cost is real. `arc_post` and `bezier_post` read every hit's blocks
--- after the scan, which takes 7 to 8 times the baseline search time.
--- `arc_pre` pays at index time instead: it writes one row per gated pair,
--- and its scan is 1.4 times the baseline. `slate_post` takes 360 to 405
--- milliseconds per query.
---
--- So enable a strategy for a measurement, and never as a default.
---
--- STRATEGIES
---
--- `points` is the identity. The plain block scan stands at both stages,
--- and every other strategy is measured against it.
---
--- `arc_post` scores the great-circle arc from a hit's vector to each
--- embedded neighbour after the scan. A hit wins when the query sits in
--- the arc's interior, and the win cites both spans.
---
--- `arc_pre` writes one normalized midpoint row per adjacent pair at index
--- time, with the kind `transition`. The plain scan then sees the row.
---
--- `bezier_post` scores a smoothed curve through `curve_window` consecutive
--- blocks after the scan, and cites every span in the window.
---
--- `meta_pre` re-embeds every embedded block at index time with a prefix:
--- the note's title, the nearest heading above the block, and the note's
--- `description` property. `meta_format` sets the layout of the text that
--- reaches the embedder. The parser's rows keep their spans and text; only
--- the vector changes. The title and the description come with the event.
--- The strategy needs a named kiln and a bound `cru.embed`.
---
--- `slate_post` keeps a signal per candidate (point score, interior arc,
--- link hop) and composes the list by quota: the `slate` plan names the
--- slots in order, one row per note across them, then the rest follows by
--- point score. The counters `slots_point`, `slots_arc` and `slots_hop`
--- count the rows each type placed.
---
--- `ppr_post` runs a personalized PageRank over the link graph of the first
--- searched kiln after the scan. The teleport vector is the notes the scan
--- hit, each with its best dense score, scaled to sum to one; no mass
--- teleports elsewhere, so the walk stays personal and a hub note gains
--- only through a seed. Twenty rounds at `ppr_damping`, then the ranks are
--- scaled so the highest is one. Each hit gains `ppr_blend` times
--- its note's rank. The `ppr_extra` best notes the scan did not hit enter
--- the list through their first embedded block, with the score
--- `ppr_blend` times their rank. The graph is read once per kiln and
--- forgotten on every note event.
---
--- `meta_ppr` is `meta_pre` at index time and `ppr_post` after the scan.

local config = require("config")

type Block = { span_start: number, span_end: number, kind: string, vector: { number }? }
type Span = { number }

local M = {}

-- ============================================================================
-- Counters
-- ============================================================================

local counters: { [string]: number? } = { queries = 0, interior_wins = 0 }
;(_G :: any).RETRIEVAL_LAB_COUNTERS = counters

function M.counters()
    return counters
end

function M.reset_counters()
    counters.queries = 0
    counters.interior_wins = 0
    counters.slots_point = nil
    counters.slots_arc = nil
    counters.slots_hop = nil
end

-- ============================================================================
-- Pure helpers (unit-testable without a daemon)
-- ============================================================================

--- True when the configured `exclude_kinds` names `kind`.
function M.excluded(kind: string): boolean
    local kinds = config.get("exclude_kinds")
    if type(kinds) ~= "table" then return false end
    for _, name in ipairs(kinds) do
        if name == kind then return true end
    end
    return false
end

--- A block a strategy may use: it has a vector and its kind is not excluded.
function M.usable(block: Block): boolean
    return block.vector ~= nil and not M.excluded(block.kind)
end

--- The usable blocks of a note, in span order.
function M.usable_blocks(blocks: { Block }): { Block }
    local out = {}
    for _, block in ipairs(blocks) do
        if M.usable(block) then
            out[#out + 1] = block
        end
    end
    return out
end

--- The position of the hit's own row among `blocks`, by `span_start`.
function M.position_of(hit: any, blocks: { Block }): number?
    for i, block in ipairs(blocks) do
        if block.span_start == hit.span_start then
            return i
        end
    end
    return nil
end

--- The hit's own usable row and its usable neighbours on each side.
--- Excluded kinds and rows without a vector are stepped over.
function M.embedded_neighbours(hit: any, blocks: { Block }): (Block?, Block?, Block?)
    local usable = M.usable_blocks(blocks)
    local at = M.position_of(hit, usable)
    if at == nil then return nil, nil, nil end
    return usable[at], usable[at - 1], usable[at + 1]
end

--- Every pair of adjacent usable blocks whose cosine reaches `min`.
function M.adjacent_embedded_pairs(blocks: { Block }, min: number): { { Block } }
    local pairs_out = {}
    local prev: Block? = nil
    for _, block in ipairs(M.usable_blocks(blocks)) do
        if prev ~= nil and cru.vec.cosine(prev.vector :: { number }, block.vector :: { number }) >= min then
            pairs_out[#pairs_out + 1] = { prev, block }
        end
        prev = block
    end
    return pairs_out
end

--- Up to `size` usable blocks around the hit, in span order. The window
--- stops at a pair whose cosine is under `min`, so a curve never crosses a
--- topic break.
function M.window_around(hit: any, blocks: { Block }, size: number, min: number): { Block }
    local usable = M.usable_blocks(blocks)
    local at = M.position_of(hit, usable)
    if at == nil then return {} end
    local passes = function(a: Block, b: Block): boolean
        return cru.vec.cosine(a.vector :: { number }, b.vector :: { number }) >= min
    end
    local lo, hi = at, at
    while hi - lo + 1 < size do
        local grew = false
        if lo > 1 and passes(usable[lo - 1], usable[lo]) then
            lo -= 1
            grew = true
        end
        if hi - lo + 1 < size and hi < #usable and passes(usable[hi], usable[hi + 1]) then
            hi += 1
            grew = true
        end
        if not grew then break end
    end
    local window = {}
    for i = lo, hi do
        window[#window + 1] = usable[i]
    end
    return window
end

function M.vectors(blocks: { Block }): { { number } }
    local out = {}
    for _, block in ipairs(blocks) do
        out[#out + 1] = block.vector :: { number }
    end
    return out
end

function M.spans(blocks: { Block }): { Span }
    local out = {}
    for _, block in ipairs(blocks) do
        out[#out + 1] = { block.span_start, block.span_end }
    end
    return out
end

function M.add(a: { number }, b: { number }): { number }
    local out = {}
    for i = 1, #a do
        out[i] = a[i] + b[i]
    end
    return out
end

--- Hits by score, best first; ties keep the merged order. An introduced
--- entry has no `index` and sorts after a merged hit of the same score.
function M.sorted(hits: { any }): { any }
    local out = {}
    for _, hit in ipairs(hits) do
        out[#out + 1] = hit
    end
    table.sort(out, function(a, b)
        if a.score ~= b.score then return a.score > b.score end
        return (a.index or math.huge) < (b.index or math.huge)
    end)
    return out
end

-- ============================================================================
-- Block access
-- ============================================================================

--- The stored rows of the hit's note, fetched once per note per event. A
--- whole-note hit and a hit from a nameless kiln answer nothing.
function M.blocks_for(hit: any, cache: { [string]: { Block } }): { Block }?
    if type(hit.kiln) ~= "string" or type(hit.path) ~= "string" or hit.span_start == nil then
        return nil
    end
    local key = hit.kiln .. "\0" .. hit.path
    local cached = cache[key]
    if cached ~= nil then return cached end
    local ok, blocks = pcall(cru.kiln.blocks, hit.kiln, hit.path)
    if not ok or type(blocks) ~= "table" then
        cru.log("warn", "retrieval-lab: cru.kiln.blocks failed for " .. hit.path .. ": " .. tostring(blocks))
        return nil
    end
    cache[key] = blocks
    return blocks
end

--- The first usable stored row of a note, or nil.
function M.first_usable_block(kiln: string, path: string): Block?
    local ok, blocks = pcall(cru.kiln.blocks, kiln, path)
    if not ok or type(blocks) ~= "table" then
        cru.log("warn", "retrieval-lab: cru.kiln.blocks failed for " .. path .. ": " .. tostring(blocks))
        return nil
    end
    return M.usable_blocks(blocks)[1]
end

-- ============================================================================
-- Metadata prefix (meta_pre)
-- ============================================================================

--- A heading's text without its `#` marks.
function M.heading_text(text: string): string
    return (text:gsub("^#+%s*", ""):gsub("%s+$", ""))
end

--- The text `meta_pre` embeds for one block. `{meta}` is the title and the
--- heading joined by ` > `, with the description on its own line; `{text}`
--- is the block's own text.
function M.meta_text(title: string, heading: string, description: string, text: string): string
    local parts = {}
    if title ~= "" then parts[#parts + 1] = title end
    if heading ~= "" then parts[#parts + 1] = heading end
    local meta = table.concat(parts, " > ")
    if description ~= "" then
        meta = if meta == "" then description else meta .. "\n" .. description
    end
    local format: string = config.get("meta_format")
    local out = format:gsub("{meta}", function() return meta end)
    return (out:gsub("{text}", function() return text end))
end

-- ============================================================================
-- Link graph (ppr_post)
-- ============================================================================

type Graph = { paths: { string }, neighbours: { [string]: { string } } }

-- One graph per kiln name, kept until a note event clears the table.
local graphs: { [string]: Graph } = {}

function M.forget_graphs()
    graphs = {}
end

--- The undirected link graph of a named kiln, read through `cru.kiln.notes`
--- and `cru.kiln.links` once, then cached. A link to a path the listing
--- does not hold is dropped.
function M.graph_for(kiln: string): Graph?
    local cached = graphs[kiln]
    if cached ~= nil then return cached end
    local ok, notes = pcall(cru.kiln.notes, kiln)
    if not ok or type(notes) ~= "table" then
        cru.log("warn", "retrieval-lab: cru.kiln.notes failed for " .. kiln .. ": " .. tostring(notes))
        return nil
    end
    local graph: Graph = { paths = {}, neighbours = {} }
    for _, note in ipairs(notes) do
        if graph.neighbours[note.path] == nil then
            graph.paths[#graph.paths + 1] = note.path
            graph.neighbours[note.path] = {}
        end
    end
    local function connect(a: string, b: string)
        local list = graph.neighbours[a]
        if list == nil or a == b then return end
        for _, existing in ipairs(list) do
            if existing == b then return end
        end
        list[#list + 1] = b
    end
    for _, path in ipairs(graph.paths) do
        local ok_links, links = pcall(cru.kiln.links, kiln, path)
        if ok_links and type(links) == "table" and type(links.outlinks) == "table" then
            for _, target in ipairs(links.outlinks) do
                if graph.neighbours[target] ~= nil then
                    connect(path, target)
                    connect(target, path)
                end
            end
        end
    end
    graphs[kiln] = graph
    return graph
end

--- Personalized PageRank over `graph`. `seeds` maps a path to its teleport
--- mass; the mass the seeds leave under one teleports uniformly, and seeds
--- over one are scaled down to one. A node with no neighbour hands its
--- mass back to the teleport. The result is scaled so the highest rank is
--- one.
function M.pagerank(graph: Graph, seeds: { [string]: number }, damping: number, iterations: number): { [string]: number }
    local n = #graph.paths
    if n == 0 then return {} end
    local total = 0
    for _, mass in pairs(seeds) do
        total += mass
    end
    if total <= 0 then return {} end
    local teleport: { [string]: number } = {}
    local rank: { [string]: number } = {}
    for _, path in ipairs(graph.paths) do
        teleport[path] = (seeds[path] or 0) / total
        rank[path] = teleport[path]
    end
    for _ = 1, iterations do
        local flow: { [string]: number } = {}
        local lost = 0
        for _, path in ipairs(graph.paths) do
            flow[path] = 0
        end
        for _, path in ipairs(graph.paths) do
            local list = graph.neighbours[path]
            if #list == 0 then
                lost += rank[path]
            else
                local share = rank[path] / #list
                for _, neighbour in ipairs(list) do
                    flow[neighbour] += share
                end
            end
        end
        for _, path in ipairs(graph.paths) do
            rank[path] = (1 - damping) * teleport[path] + damping * (flow[path] + lost * teleport[path])
        end
    end
    local top = 0
    for _, value in pairs(rank) do
        if value > top then top = value end
    end
    if top > 0 then
        for path, value in pairs(rank) do
            rank[path] = value / top
        end
    end
    return rank
end

-- ============================================================================
-- Strategies
-- ============================================================================

-- ============================================================================
-- Match signals and the slate (slate_post)
-- ============================================================================

--- The best interior arc between the hit's block and an embedded
--- neighbour: the score and the two spans, or nil when no neighbour passes
--- the gate or the interior does not beat the point. An endpoint (t = 0 or
--- 1) is a point score the scan already has.
function M.arc_signal(hit: any, blocks: { Block }, query: { number }, min: number): (number?, { Span }?)
    local own, prev, next = M.embedded_neighbours(hit, blocks)
    if own == nil then return nil, nil end
    local neighbours: { Block } = {}
    if prev ~= nil then neighbours[#neighbours + 1] = prev end
    if next ~= nil then neighbours[#neighbours + 1] = next end
    local best: number? = nil
    local cited: { Span }? = nil
    for _, nb in ipairs(neighbours) do
        if cru.vec.cosine(own.vector :: { number }, nb.vector :: { number }) >= min then
            local score, t = cru.vec.arc_best(query, own.vector :: { number }, nb.vector :: { number })
            if t > 0 and t < 1 and score > hit.score and (best == nil or score > best) then
                best = score
                cited = { { own.span_start, own.span_end }, { nb.span_start, nb.span_end } }
            end
        end
    end
    return best, cited
end

--- The best block of every note one link from a note in `from`, by cosine
--- with the query, best first. Notes in `skip` are left out. Each entry is
--- a row the rerank may introduce: `{ kiln, path, span_start, span_end, score }`.
function M.hop_signal(kiln: string, from: { string }, query: { number }, skip: { [string]: boolean }): { any }
    local graph = M.graph_for(kiln)
    if graph == nil then return {} end
    local seen: { [string]: boolean } = {}
    local out: { any } = {}
    for _, path in ipairs(from) do
        for _, nb in ipairs(graph.neighbours[path] or {}) do
            if not seen[nb] and not skip[nb] then
                seen[nb] = true
                local ok, blocks = pcall(cru.kiln.blocks, kiln, nb)
                if ok and type(blocks) == "table" then
                    local best: Block? = nil
                    local best_score = -1
                    for _, block in ipairs(M.usable_blocks(blocks)) do
                        local c = cru.vec.cosine(query, block.vector :: { number })
                        if c > best_score then
                            best_score = c
                            best = block
                        end
                    end
                    if best ~= nil then
                        out[#out + 1] = { kiln = kiln, path = nb, span_start = best.span_start, span_end = best.span_end, score = best_score }
                    end
                end
            end
        end
    end
    table.sort(out, function(a, b)
        if a.score ~= b.score then return a.score > b.score end
        return a.path < b.path
    end)
    return out
end

local strategies: { [string]: any } = {}

-- Identity. The baseline every other strategy is measured against.
strategies.points = {
    rerank = function(event) return nil end,
    index = function(event) return nil end,
}

-- Great-circle arc to each embedded neighbour, after the scan.
strategies.arc_post = {
    rerank = function(event: any): any
        local cache = {}
        local min = config.get("pair_cosine_min")
        for _, hit in ipairs(event.hits) do
            local blocks = M.blocks_for(hit, cache)
            if blocks ~= nil then
                local score, cited = M.arc_signal(hit, blocks, event.query_vector, min)
                if score ~= nil then
                    hit.score = score
                    hit.cited = cited
                end
            end
        end
        return M.sorted(event.hits)
    end,
    index = function(event) return nil end,
}

-- Normalized midpoints written at index time; the plain scan sees them.
-- A row's `span_start` is its primary key and `a.span_start` is taken, so
-- a transition row starts one byte into `a`. No block starts there, and
-- the span still overlaps both blocks, so the row quotes both.
strategies.arc_pre = {
    rerank = function(event) return nil end,
    index = function(event)
        local extra = {}
        local min = config.get("pair_cosine_min")
        for _, pair in ipairs(M.adjacent_embedded_pairs(event.blocks, min)) do
            local a, b = pair[1], pair[2]
            extra[#extra + 1] = {
                span_start = a.span_start + 1,
                span_end = b.span_end,
                kind = "transition",
                vector = cru.vec.normalize(M.add(a.vector :: { number }, b.vector :: { number })),
            }
        end
        return { extra = extra }
    end,
}

-- A smoothed curve through `curve_window` consecutive blocks, after the scan.
strategies.bezier_post = {
    rerank = function(event)
        local cache = {}
        local min = config.get("pair_cosine_min")
        local size = config.get("curve_window")
        local samples = config.get("curve_samples")
        for _, hit in ipairs(event.hits) do
            local blocks = M.blocks_for(hit, cache)
            if blocks ~= nil then
                local window = M.window_around(hit, blocks, size, min)
                if #window >= 3 then
                    local score, t = cru.vec.curve_best(event.query_vector, M.vectors(window), samples)
                    if t > 0 and t < 1 and score > hit.score then
                        hit.score = score
                        hit.cited = M.spans(window)
                    end
                end
            end
        end
        return M.sorted(event.hits)
    end,
    index = function(event) return nil end,
}

-- Every embedded block re-embedded under the note's title, the nearest
-- heading above it and the note's description, at index time. A heading
-- row and a row without a vector keep what the pipeline gave them.
strategies.meta_pre = {
    rerank = function(event) return nil end,
    index = function(event: any): any
        if type(event.kiln) ~= "string" then return nil end
        -- The stage puts the title and the description on the event: a read
        -- through `cru.kiln.note` would wait on the connection the batch holds.
        local title: string = event.title or ""
        local description: string = event.description or ""
        local heading = ""
        local replace = {}
        for _, block in ipairs(event.blocks) do
            if block.kind == "heading" then
                heading = M.heading_text(block.text or "")
            elseif block.vector ~= nil then
                local text = M.meta_text(title, heading, description, block.text or "")
                local ok, vector = pcall(cru.embed, event.kiln, text)
                if ok and type(vector) == "table" then
                    replace[#replace + 1] = { span_start = block.span_start, vector = vector }
                else
                    cru.log("warn", "retrieval-lab: cru.embed failed for " .. event.path .. ": " .. tostring(vector))
                end
            end
        end
        return { replace = replace }
    end,
}

-- A personalized PageRank from the hit notes over the link graph of the
-- first searched kiln, after the scan. Hits from another kiln keep their
-- score.
strategies.ppr_post = {
    rerank = function(event: any): any
        local kiln = event.kilns[1]
        if type(kiln) ~= "string" then return nil end
        local graph = M.graph_for(kiln)
        if graph == nil then return nil end
        local seeds: { [string]: number } = {}
        for _, hit in ipairs(event.hits) do
            if hit.kiln == kiln and type(hit.path) == "string" and hit.score > (seeds[hit.path] or 0) then
                seeds[hit.path] = hit.score
            end
        end
        if next(seeds) == nil then return nil end
        local rank = M.pagerank(graph, seeds, config.get("ppr_damping"), 20)
        local blend = config.get("ppr_blend")
        local out = {}
        for _, hit in ipairs(event.hits) do
            if hit.kiln == kiln and rank[hit.path] ~= nil then
                hit.score += blend * rank[hit.path]
            end
            out[#out + 1] = hit
        end
        -- The best notes the walk reached and the scan did not.
        local reached = {}
        for path, score in pairs(rank) do
            if seeds[path] == nil and score > 0 then
                reached[#reached + 1] = { path = path, score = score }
            end
        end
        table.sort(reached, function(a, b)
            if a.score ~= b.score then return a.score > b.score end
            return a.path < b.path
        end)
        local extra = 0
        for _, candidate in ipairs(reached) do
            if extra >= config.get("ppr_extra") then break end
            local block = M.first_usable_block(kiln, candidate.path)
            if block ~= nil then
                extra += 1
                out[#out + 1] = {
                    kiln = kiln,
                    path = candidate.path,
                    span_start = block.span_start,
                    span_end = block.span_end,
                    score = blend * candidate.score,
                }
            end
        end
        return M.sorted(out)
    end,
    index = function(event) return nil end,
}

-- The metadata prefix at index time and the PageRank after the scan.
strategies.meta_ppr = {
    rerank = strategies.ppr_post.rerank,
    index = strategies.meta_pre.index,
}

-- A slate: the list is filled by quota from each match type in turn, one
-- row per note across the quota slots, then the rest by point score. A
-- slot type is `point` (dense order), `arc` (interior pair wins, best
-- first; the row takes the arc score and cites the pair) or `hop` (the best
-- block of a note one link from the top `hop_from` dense notes; an existing
-- row of that note stands in, else the row is introduced). The `slate`
-- knob is the plan, so a config change is a new composition.
strategies.slate_post = {
    rerank = function(event: any): any
        local kiln = event.kilns[1]
        local cache = {}
        local min = config.get("pair_cosine_min")
        local plan = config.get("slate")
        if type(plan) ~= "table" then return nil end
        local query = event.query_vector

        local by_point = M.sorted(event.hits)
        local function row_key(h: any): string
            return tostring(h.kiln) .. "\0" .. tostring(h.path) .. "\0" .. tostring(h.span_start)
        end
        local function note_key(h: any): string
            return tostring(h.kiln) .. "\0" .. tostring(h.path)
        end

        local arcs: { any } = {}
        for _, hit in ipairs(event.hits) do
            local blocks = M.blocks_for(hit, cache)
            if blocks ~= nil then
                local score, cited = M.arc_signal(hit, blocks, query, min)
                if score ~= nil then
                    arcs[#arcs + 1] = { hit = hit, score = score, cited = cited }
                end
            end
        end
        table.sort(arcs, function(a, b)
            if a.score ~= b.score then return a.score > b.score end
            return a.hit.index < b.hit.index
        end)

        local hops: { any } = {}
        if type(kiln) == "string" then
            local from: { string } = {}
            local from_set: { [string]: boolean } = {}
            for _, hit in ipairs(by_point) do
                if #from >= config.get("hop_from") then break end
                if hit.kiln == kiln and type(hit.path) == "string" and not from_set[hit.path] then
                    from_set[hit.path] = true
                    from[#from + 1] = hit.path
                end
            end
            hops = M.hop_signal(kiln, from, query, from_set)
        end

        local out: { any } = {}
        local taken_rows: { [string]: boolean } = {}
        local taken_notes: { [string]: boolean } = {}
        local function place(entry: any): boolean
            if taken_rows[row_key(entry)] or taken_notes[note_key(entry)] then return false end
            taken_rows[row_key(entry)] = true
            taken_notes[note_key(entry)] = true
            out[#out + 1] = entry
            return true
        end
        for _, slot in ipairs(plan) do
            local filled = 0
            if slot.type == "point" then
                for _, hit in ipairs(by_point) do
                    if filled >= slot.n then break end
                    if place(hit) then filled += 1 end
                end
            elseif slot.type == "arc" then
                for _, arc in ipairs(arcs) do
                    if filled >= slot.n then break end
                    if not taken_rows[row_key(arc.hit)] and not taken_notes[note_key(arc.hit)] then
                        arc.hit.score = arc.score
                        arc.hit.cited = arc.cited
                        place(arc.hit)
                        filled += 1
                    end
                end
            elseif slot.type == "hop" then
                for _, hop in ipairs(hops) do
                    if filled >= slot.n then break end
                    local entry = hop
                    for _, hit in ipairs(by_point) do
                        if hit.kiln == hop.kiln and hit.path == hop.path then
                            entry = hit
                            break
                        end
                    end
                    if place(entry) then filled += 1 end
                end
            end
            local name = "slots_" .. tostring(slot.type)
            counters[name] = (counters[name] or 0) + filled
        end
        for _, hit in ipairs(by_point) do
            if not taken_rows[row_key(hit)] then
                taken_rows[row_key(hit)] = true
                out[#out + 1] = hit
            end
        end
        return out
    end,
    index = function(event) return nil end,
}

M.strategies = strategies

-- ============================================================================
-- Dispatch
-- ============================================================================

local warned_unknown = false

--- The configured strategy, or nil after one warning for a name this file
--- does not define.
function M.current(): any
    local name = config.get("strategy")
    local strategy = strategies[name]
    if strategy == nil and not warned_unknown then
        warned_unknown = true
        cru.log("warn", "retrieval-lab: unknown strategy '" .. tostring(name) .. "'; the plain scan stands")
    end
    return strategy
end

--- The `search:rerank` handler body.
function M.rerank(event: any): any
    local strategy = M.current()
    if strategy == nil then return nil end
    counters.queries = (counters.queries or 0) + 1
    local after = strategy.rerank(event)
    if type(after) == "table" then
        for i = 1, math.min(10, #after) do
            if after[i].cited ~= nil then
                counters.interior_wins = (counters.interior_wins or 0) + 1
                break
            end
        end
    end
    return after
end

--- The `index:blocks` handler body.
function M.index(event: any): any
    local strategy = M.current()
    if strategy == nil then return nil end
    return strategy.index(event)
end

local registered = false

--- Register the two handlers once. Registration alone makes every search
--- over-fetch, so a disabled plugin registers nothing.
function M.activate()
    if registered or not config.get("enabled") then return end
    registered = true
    cru.on("search:rerank", { priority = 100 }, function(ctx, event)
        return M.rerank(event)
    end)
    cru.on("index:blocks", { priority = 100 }, function(ctx, event)
        return M.index(event)
    end)
    -- A note event changes the link graph, so the cached graph goes.
    for _, name in ipairs({ "note:created", "note:modified", "note:deleted", "note:renamed" }) do
        cru.on(name, {}, function(ctx, event)
            M.forget_graphs()
        end)
    end
end

M.activate()

local plugin = {
    name = "retrieval-lab",
    version = "0.1.0",
    description = "Retrieval strategies over block trajectories, for measurement",
    capabilities = { "kiln", "config" },

    -- Exposed for unit tests.
    counters = M.counters,
    reset_counters = M.reset_counters,
    forget_graphs = M.forget_graphs,
    strategies = strategies,

    setup = function(cfg)
        if cfg then
            config.init(cfg)
        end
        M.activate()
    end,
}

-- The daemon executes this file BY PATH, so a later `require("retrieval-lab")`
-- would evaluate a second copy with its own counters and handlers. Same
-- guard as `reflection`.
package.loaded["retrieval-lab"] = plugin

return plugin
