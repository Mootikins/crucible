--!strict
--- retrieval-lab configuration
---
--- `cru.settings.new` gives the order, highest priority first: setup()
--- values, then the `plugins.retrieval-lab` section, then `defaults`, then the
--- caller's fallback. The daemon calls setup() with the section at load. A
--- later `setup{}` call from a user lays over it.

local PLUGIN = "retrieval-lab"

local M = {}

-- The keys mirror plugin.yaml. Every embedded content block joins a pair.
-- A heading is a label, and the few long enough to embed cost the
-- adversarial corpus without a gain (lab runs 3 and 4). `transition` is
-- excluded so that a synthetic row an earlier `arc_pre` index wrote never
-- counts as a neighbour.
local defaults = {
    enabled = false,
    strategy = "points",
    pair_cosine_min = 0.5,
    curve_window = 3,
    curve_samples = 8,
    exclude_kinds = { "heading", "transition" },
    -- meta_pre: `{meta}` is the title, the heading and the description;
    -- `{text}` is the block's own text.
    meta_format = "{meta}\n\n{text}",
    -- ppr_post: the teleport mass of a hit note is its best dense score
    -- times the seed weight. The rest of the mass teleports uniformly.
    ppr_damping = 0.5,
    ppr_blend = 0.1,
    ppr_extra = 3,
    -- slate_post: the quota plan, in order, then the rest by point score.
    slate = { { type = "point", n = 2 }, { type = "arc", n = 2 }, { type = "hop", n = 2 } },
    hop_from = 2,
}

local settings = cru.settings.new(PLUGIN, defaults)

M.init = settings.init
M.get = settings.get
M.reset = settings.reset

return M
