# Graph View Plugin

A knowledge graph visualization plugin for Crucible written in Fennel.

## Features

- **graph** - Interactive knowledge graph view with keyboard navigation
- **graph_stats** - Get statistics about the knowledge graph
- **/graph** - Command to open the graph view

## Installation

Copy this plugin to your Crucible plugins directory:

```bash
cp -r graph-view ~/.config/crucible/plugins/
# or
cp -r graph-view ~/your-kiln/plugins/
```

## Usage

### View (for users)

```
/graph              # Open graph centered on current note
/graph [[Index]]    # Open graph centered on specific note
```

**Keyboard shortcuts in graph view:**
- `j` / `k` - Navigate up/down through nodes
- `Enter` - Focus on selected node (re-center graph)
- `r` - Refresh graph
- `q` - Close view

### Tool (for agents)

```lua
-- Get graph statistics
graph_stats({ note = "Index", depth = 3 })
-- Returns: { center, total_nodes, total_edges, max_depth, nodes_by_depth }
```

### Command (for users)

```
/graph              # Open graph view for current note
/graph Index        # Open graph view centered on "Index"
```

## Configuration

In your `crucible.toml`:

```toml
[plugins.graph-view]
max_depth = 3        # Maximum traversal depth
show_orphans = false # Show unlinked notes
layout = "force"     # Layout algorithm: force, tree, radial
```

Or in `plugin.yaml`:

```yaml
config:
  properties:
    max_depth:
      type: number
      default: 3
    show_orphans:
      type: boolean
      default: false
    layout:
      type: string
      default: "force"
```

## Graph Structure

The graph view shows:
- **Center node** (yellow, marked with `*`)
- **Outgoing links** (white, marked with `->`)
- **Incoming links/backlinks** (magenta, marked with `<-`)

Nodes are indented by depth from the center node.

## Fennel Implementation

This plugin demonstrates Fennel syntax for Crucible plugins:

```fennel
;;; Description shown in help
;; @view name="graph" desc="Knowledge graph view"
(fn graph_view [ctx]
  (cru.oil.col {:gap 1}
    (cru.oil.text "Graph View" {:bold true})))
```

Key Fennel features used:
- `let` bindings for local variables
- `when` for conditional expressions
- `each` for iteration
- Table destructuring
- The `cru` global for Crucible APIs

## Capabilities Required

- `ui` - Render custom views with cru.oil
- `kiln` - Access note links via cru.kiln.outlinks/backlinks

## API Reference

### cru.kiln.outlinks(note)

Returns array of note names that the given note links to.

### cru.kiln.backlinks(note)

Returns array of note names that link to the given note.

### cru.oil.text(content, style)

Render styled text. Style options: `bold`, `fg`, `bg`.

### cru.oil.col(opts, ...children)

Vertical layout container. Options: `gap` (spacing between children).
