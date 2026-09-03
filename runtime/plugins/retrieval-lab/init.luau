--!strict
--- retrieval-lab — retrieval strategies over block trajectories.
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
--- `ppr_post` runs a personalized PageRank over the link graph of the first
--- searched kiln after the scan. The seeds are the notes the scan hit, each
--- with `ppr_seed_weight` times its best dense score; the mass the seeds
--- leave teleports uniformly. Twenty rounds at `ppr_damping`, then the
--- ranks are scaled so the highest is one. Each hit gains `ppr_blend` times
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

local counters = { queries = 0, interior_wins = 0 }
;(_G :: any).RETRIEVAL_LAB_COUNTERS = counters

function M.counters()
    return counters
end

function M.reset_counters()
    counters.queries = 0
    counters.interior_wins = 0
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
    local uniform = math.max(0, 1 - total) / n
    local scale = if total > 1 then 1 / total else 1
    local teleport: { [string]: number } = {}
    local rank: { [string]: number } = {}
    for _, path in ipairs(graph.paths) do
        teleport[path] = uniform + (seeds[path] or 0) * scale
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

local strategies: { [string]: any } = {}

-- Identity. The baseline every other strategy is measured against.
strategies.points = {
    rerank = function(event) return nil end,
    index = function(event) return nil end,
}

-- Great-circle arc to each embedded neighbour, after the scan.
strategies.arc_post = {
    rerank = function(event)
        local cache = {}
        local min = config.get("pair_cosine_min")
        for _, hit in ipairs(event.hits) do
            local blocks = M.blocks_for(hit, cache)
            if blocks ~= nil then
                local own, prev, next = M.embedded_neighbours(hit, blocks)
                local neighbours = {}
                if prev ~= nil then neighbours[#neighbours + 1] = prev end
                if next ~= nil then neighbours[#neighbours + 1] = next end
                if own ~= nil then
                    for _, nb in ipairs(neighbours) do
                        if cru.vec.cosine(own.vector :: { number }, nb.vector :: { number }) >= min then
                            local score, t = cru.vec.arc_best(event.query_vector, own.vector :: { number }, nb.vector :: { number })
                            -- An endpoint (t = 0 or 1) is a point score the
                            -- scan already has; only the interior is a win.
                            if t > 0 and t < 1 and score > hit.score then
                                hit.score = score
                                hit.cited = { { own.span_start, own.span_end }, { nb.span_start, nb.span_end } }
                            end
                        end
                    end
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
        local weight = config.get("ppr_seed_weight")
        local seeds: { [string]: number } = {}
        for _, hit in ipairs(event.hits) do
            if hit.kiln == kiln and type(hit.path) == "string" then
                local mass = hit.score * weight
                if mass > (seeds[hit.path] or 0) then
                    seeds[hit.path] = mass
                end
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
    counters.queries += 1
    local after = strategy.rerank(event)
    if type(after) == "table" then
        for i = 1, math.min(10, #after) do
            if after[i].cited ~= nil then
                counters.interior_wins += 1
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
