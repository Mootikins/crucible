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
web traverses it there. No route serves a neighbourhood. `cru.kiln.neighbors`
does — scope-filtered per hop and cycle-safe — and Lua is its only caller. So
this plugin sends the small answer instead of the large input, which is the
placement rule in `docs/Meta/Analysis/The Plugin Contract.md`: *reduce where
the data is*.

`crucible-web/web/src/components/blocks/GraphBlock.tsx` draws it, and re-asks
on every move of the depth control. That is the latency question
`docs/Meta/Analysis/Plugin API Plan.md` step 2 wanted measured.

## Rings, not edges

`cru.kiln.neighbors(path, depth)` is cumulative and attaches no distance, so
one call cannot say how far away each note is. This walks one call per hop and
differences each ring against the nearer ones.

A ring carries paths, not `{source, target}` pairs, because the Lua surface has
no bulk edge read. `cru.kiln.outlinks(path)` answers for one note and each call
re-reads the whole scoped note list plus the whole link table, so edges among N
returned notes would cost N full graph scans against this walk's `depth` scans.
An edge view wants a bulk primitive first.

## What it cost

On a 2 000-note, 12 000-edge kiln one depth-1 read takes 80 ms and a depth-4
read 325 ms, against 137 ms to fetch the whole graph over the same socket
**once**. The reduction saves a megabyte on the wire and saves the daemon
nothing: `cru.kiln.neighbors` reads the whole scoped note list plus the whole
link table on every call. Numbers, method and what the primitive wants instead
are in `docs/Meta/Analysis/Plugin API Plan.md`, step 2.

## Embed it

    ```plugin
    graph/neighborhood
    { "path": "Meta/Analysis/Canvas.md", "depth": 2 }
    ```

With no `path` the block follows whichever note has focus in the editor. It is
also available as **Plugin Blocks → graph / neighborhood** in the right dock.
