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

--- Hits by score, best first; ties keep the merged order.
function M.sorted(hits: { any }): { any }
    local out = {}
    for _, hit in ipairs(hits) do
        out[#out + 1] = hit
    end
    table.sort(out, function(a, b)
        if a.score ~= b.score then return a.score > b.score end
        return a.index < b.index
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
