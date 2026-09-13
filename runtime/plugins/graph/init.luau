--!strict
--- graph — the neighbourhood of one note, read on the daemon.
---
--- This plugin exists to answer one question the plugin contract keeps
--- asking: **where does a parameterised read belong?**
---
--- `GET /api/kiln/graph` hands the browser the WHOLE edge list of a kiln and
--- the web traverses it there. That is the right shape for a small kiln and
--- the wrong one for a large kiln, and no HTTP route serves a neighbourhood.
--- `cru.kiln.neighbors_with_hops(path, depth)` does serve one —
--- scope-filtered, cycle-safe, and Lua is the only caller that can reach it.
--- So the reduction happens here, on the daemon, and a client receives the
--- small answer rather than the large input.
---
--- **Rings, not a flat list.** The read answers every neighbour within
--- `depth` with the hop count the walk reached it at, sorted hop-major, so a
--- ring is a run of rows. That is a reduction a browser could only perform by
--- holding the whole edge list — which is the thing this plugin exists to
--- avoid sending.
---
--- This used to call `cru.kiln.neighbors(path, hop)` once per depth and
--- difference the answers, because that read is cumulative and attaches no
--- distance. Each call re-read the whole note list and the whole link table
--- daemon-side. The walk always knew the hop count and discarded it.
---
--- **Why no edges.** A ring carries paths, not `{source, target}` pairs. The
--- Lua surface has no bulk edge read: `cru.kiln.outlinks(path)` answers for
--- ONE note and each call re-reads the whole scoped note list plus the whole
--- link table (`crucible-core/src/storage/scoped_links.rs`). Deriving edges
--- among N returned notes therefore costs N full graph scans, against the
--- `depth` scans this walk costs. An edge view wants a bulk primitive first;
--- it does not want an N+1 loop written in Luau.

local M = {}

--- Hops to walk when the caller names none.
local DEFAULT_DEPTH = 1
--- The largest walk this plugin will perform. Each hop is a full scan of the
--- scoped link table, so an unbounded `depth` from a client is an unbounded
--- amount of daemon work per keystroke of a slider.
local MAX_DEPTH = 4
--- Notes returned before the answer is cut short.
local DEFAULT_LIMIT = 200
local MAX_LIMIT = 2000

local config = {
    depth = DEFAULT_DEPTH,
    limit = DEFAULT_LIMIT,
}

--- One hop's worth of notes: everything exactly `hops` steps from the root.
export type Ring = {
    hops: number,
    paths: { string },
}

--- The whole answer, in one shape whether the read worked or not.
---
--- A command that returns `{ok=false}` on failure and a data table on success
--- makes every consumer branch on the shape before it can read a field. One
--- shape with an optional `error` reads the same either way, and a renderer
--- that ignores `error` still draws an empty neighbourhood rather than
--- throwing.
export type Neighborhood = {
    root: string,
    depth: number,
    total: number,
    truncated: boolean,
    rings: { Ring },
    error: string?,
}

function M.setup(cfg)
    if type(cfg) == "table" then
        if type(cfg.depth) == "number" then config.depth = cfg.depth end
        if type(cfg.limit) == "number" then config.limit = cfg.limit end
    end
    -- Publish the read's own description, so the block appears in the Plugin
    -- Blocks index alongside plugins that publish data. It is a descriptor,
    -- not the data: the neighbourhood depends on which note has focus, which
    -- the daemon does not know, so it cannot be published ahead of the ask.
    --
    -- `pcall` because `cru.plugin` is bound per plugin by the loader, and this
    -- file also runs on the bare VM that `cru plugin check` and the Lua test
    -- runner use.
    pcall(function()
        cru.plugin.publish("graph:neighborhood", M.describe())
    end)
end

local function clamp(value: number, low: number, high: number): number
    if value < low then return low end
    if value > high then return high end
    return math.floor(value)
end

--- What this plugin offers, and the defaults a client should start from.
function M.describe()
    return {
        command = "graph_neighborhood",
        depth = clamp(config.depth, 1, MAX_DEPTH),
        max_depth = MAX_DEPTH,
        limit = clamp(config.limit, 1, MAX_LIMIT),
    }
end

--- Notes within `depth` hops of `path`, grouped by how far away they are.
---
--- `cru.kiln.neighbors_with_hops` filters the walk against what this daemon's
--- authority may read, so a note the caller cannot see is absent from every
--- ring and cannot be reached THROUGH either. Nothing here re-checks that, and nothing
--- here should: the scope answer belongs to the store.
function M.neighborhood(path: string?, depth: number?, limit: number?): Neighborhood
    local root = if type(path) == "string" then path else ""
    local hops = clamp(if type(depth) == "number" then depth else config.depth, 1, MAX_DEPTH)
    local cap = clamp(if type(limit) == "number" then limit else config.limit, 1, MAX_LIMIT)

    local answer: Neighborhood = {
        root = root,
        depth = hops,
        total = 0,
        truncated = false,
        rings = {},
    }

    if root == "" then
        answer.error = "a `path` is required"
        return answer
    end

    -- One call, not one per hop. `neighbors_with_hops` returns the hop count
    -- the walk already computes, so the rings are a grouping of one answer.
    -- The old loop called `cru.kiln.neighbors(root, hop)` for each depth, and
    -- every call re-read the whole note list and the whole link table
    -- daemon-side — measured at about 80 ms on a 2000-note kiln.
    local ok, reached = pcall(cru.kiln.neighbors_with_hops, root, hops)
    if not ok then
        answer.error = tostring(reached)
        return answer
    end
    if type(reached) ~= "table" then
        return answer
    end

    -- Sorted by hop then by path daemon-side, so a ring is a run of rows and
    -- the paths inside it are already in order.
    local current_hop = 0
    local ring: { string } = {}
    local function close_ring()
        if current_hop > 0 then
            table.insert(answer.rings, { hops = current_hop, paths = ring })
        end
    end

    for _, row in reached do
        if type(row) ~= "table" or type(row.path) ~= "string" then continue end
        -- The root never appears: the walk removes it, so a self-link and a
        -- cycle both fail to bring it back.
        if row.path == root then continue end
        if answer.total >= cap then
            answer.truncated = true
            break
        end
        if row.hops ~= current_hop then
            close_ring()
            current_hop = row.hops
            ring = {}
        end
        answer.total += 1
        table.insert(ring, row.path)
    end
    close_ring()

    return answer
end

function M.neighborhood_command(args: any): Neighborhood
    if type(args) ~= "table" then
        return M.neighborhood(nil, nil, nil)
    end
    return M.neighborhood(args.path, args.depth, args.limit)
end

return {
    setup = M.setup,

    tools = {
        graph_neighborhood = {
            desc = "Notes within N hops of a note, grouped by hop distance",
            params = {
                { name = "path", type = "string", desc = "Kiln-relative note path" },
                { name = "depth", type = "number", desc = "Hops to walk, 1-4", optional = true },
                { name = "limit", type = "number", desc = "Most notes to return", optional = true },
            },
            fn = M.neighborhood_command,
        },
    },

    -- A COMMAND, not only a tool. `POST /api/plugins/command` reaches a
    -- command; it cannot reach a tool, and a tool is for an agent. A block in
    -- the browser is a frontend, so the command is the half that matters here.
    --
    -- `params` and `hint` are both here on purpose. The hint is one line of
    -- free text for a person; the params are the typed declaration a web
    -- dialog is GENERATED from, and no dialog can be drawn from
    -- "<path> [depth] [limit]".
    --
    -- `effect = "read"` says this walk changes nothing a user could lose. It
    -- is a claim this plugin makes about itself and nothing verifies it — see
    -- `crates/crucible-lua/src/command_effect.rs`.
    commands = {
        graph_neighborhood = {
            desc = "Notes within N hops of a note, grouped by hop distance",
            hint = "<path> [depth] [limit]",
            effect = "read",
            params = {
                { name = "path", type = "string", desc = "Kiln-relative note path" },
                { name = "depth", type = "number", desc = "Hops to walk, 1-4", optional = true },
                { name = "limit", type = "number", desc = "Most notes to return", optional = true },
            },
            fn = M.neighborhood_command,
        },
    },
}
