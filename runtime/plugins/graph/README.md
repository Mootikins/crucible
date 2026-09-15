# graph

The neighbourhood of one note, reduced on the daemon.

`graph_neighborhood` answers "which notes are within N hops of this one, and
how far away is each" for a note you name. It is a **command**, so a frontend
can invoke it over `POST /api/plugins/command`; it is also a tool, so an agent
can call it.

    { "path": "Meta/Analysis/Canvas.md", "depth": 2, "limit": 200 }

The answer has one shape whether the read worked or not:

    {
      "root": "Meta/Analysis/Canvas.md",
      "depth": 2,
      "total": 37,
      "truncated": false,
      "rings": [
        { "hops": 1, "paths": ["..."] },
        { "hops": 2, "paths": ["..."] }
      ],
      "error": null
    }

`depth` is clamped to 1-4 and `limit` to 1-2000. `[plugins.graph]` sets the
defaults for both.

## Why this plugin exists

It is the smallest honest test of one question: **does a parameterised read
belong behind a plugin command?**

`GET /api/kiln/graph` hands the browser the whole edge list of a kiln, and the
web traverses it there. No route serves a neighbourhood.
`cru.kiln.neighbors_with_hops` does — scope-filtered, cycle-safe — and Lua is
its only caller. So
this plugin sends the small answer instead of the large input, which is the
placement rule in `docs/Meta/Analysis/The Plugin Contract.md`: *reduce where
the data is*.

`crucible-web/web/src/components/blocks/GraphBlock.tsx` draws it, and re-asks
on every move of the depth control. That is the latency question
`docs/Meta/Analysis/Plugin API Plan.md` records with historical measurements.

## Rings, not edges

`cru.kiln.neighbors_with_hops(path, depth)` answers every neighbour within
`depth` with the hop count the walk reached it at, sorted hop-major. A ring is
therefore a run of rows, and this groups one answer.

It used to call `cru.kiln.neighbors(path, hop)` once per depth and difference
each ring against the nearer ones, because that read is cumulative and
attaches no distance. Each call re-read the whole note list and the whole link
table. The walk always knew the hop count and threw it away.

A ring carries paths, not `{source, target}` pairs, because the Lua surface has
no bulk edge read. `cru.kiln.outlinks(path)` answers for one note and each call
re-reads the whole scoped note list plus the whole link table, so edges among N
returned notes would cost N full graph scans against this walk's one.
An edge view wants a bulk primitive first.

## What it cost

The original spike on a 2 000-note, 12 000-edge kiln measured a depth-1 read
at 80 ms and a depth-4 read at 325 ms, against 137 ms for the whole graph
**once**. The reduction saves a megabyte on the wire and saves the daemon
nothing: `cru.kiln.neighbors` reads the whole scoped note list plus the whole
link table on every call. Numbers, method and what the primitive wants instead
are in `docs/Meta/Analysis/Plugin API Plan.md`. These are historical debug-build
measurements from before the hop-count primitive removed repeated calls, not
current latency promises.

## Embed it

    ```plugin
    graph/neighborhood
    { "path": "Meta/Analysis/Canvas.md", "depth": 2 }
    ```

With no `path` the block follows whichever note has focus in the editor. It is
also available as **Plugin Blocks → graph / neighborhood** in the right dock.
