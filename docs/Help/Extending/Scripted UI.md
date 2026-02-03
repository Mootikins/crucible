---
description: Create interactive popups and panels from scripts
status: implemented
tags:
  - scripting
  - ui
  - popup
  - panel
  - lua
---

# Scripted UI

Scripts can display interactive UI elements — popups for selection and panels for structured choices — that work in both TUI and Web interfaces.

## Concepts

### PopupRequest vs InteractivePanel

Two primitives are available:

| Type | Use Case | Features |
|------|----------|----------|
| **PopupRequest** | Quick selections with optional free-text | Simple entries with labels and descriptions |
| **InteractivePanel** | Structured choices with hints | Filtering, multi-select, custom data |

Use `PopupRequest` for simple "pick one" scenarios. Use `InteractivePanel` for richer interactions like confirmation dialogs, multi-select, or searchable lists.

## Lua API

### PopupRequest

```lua
local popup = require("cru.popup")  -- or require("crucible.popup")

-- Create entries
local entries = {
    popup.entry("Daily Note", "Today's journal"),
    popup.entry("Todo List"),
}

-- Basic popup
local request = popup.request("Select a note", entries)

-- Allow free-text input
local search = popup.request_with_other("Search or select", entries)
```

### InteractivePanel

```lua
local ui = require("cru.ui")  -- or require("crucible.ui")

-- Create panel items
local items = {
    ui.panel_item("PostgreSQL", "Full-featured RDBMS"),
    ui.panel_item("SQLite", "Embedded, single-file"),
}

-- Basic panel
local db_panel = ui.panel("Select database", items)

-- Convenience functions
local confirmed = ui.confirm("Delete this file?")
local choice = ui.select("Pick one", {"A", "B", "C"})
local choices = ui.multi_select("Pick many", {"X", "Y"})
```

### Panel Hints

Control panel behavior with hints:

```lua
local ui = require("cru.ui")  -- or require("crucible.ui")

-- Create hints
local hints = ui.panel_hints()
    :filterable()      -- Enable search/filter
    :multi_select()    -- Allow multiple selections
    :allow_other()     -- Allow free-text input

-- Panel with hints
local panel = ui.panel_with_hints("Choose", items, hints)
```

## Handling Results

### PopupResponse

When user selects from a popup:

```lua
if response.selected_index then
    handle_selection(response.selected_index, response.selected_entry)
elseif response.other then
    handle_text(response.other)
else
    handle_dismiss()
end
```

### PanelResult

When user interacts with a panel:

```lua
if result.cancelled then
    handle_cancel()
elseif result.other then
    handle_text(result.other)
else
    for _, idx in ipairs(result.selected) do
        handle_selection(idx)
    end
end
```

## Example: Database Selector Tool

A complete example showing a panel-based tool:

```lua
-- database_selector.lua
local ui = require("cru.ui")  -- or require("crucible.ui")

--- Select a database type for your project
-- @tool name="choose_database" description="Select a database type for your project"
function choose_database(args)
    local items = {
        ui.panel_item("PostgreSQL", "Full-featured, ACID-compliant RDBMS"),
        ui.panel_item("SQLite", "Embedded, zero-configuration"),
        ui.panel_item("SurrealDB", "Multi-model with graph queries"),
    }

    local panel = ui.panel("Select database", items)

    -- Display panel and get result
    local result = cru.show_panel(panel)

    if result.cancelled then
        return { message = "Cancelled" }
    else
        local selected = items[result.selected[1]].label
        return { message = "You chose: " .. selected }
    end
end
```

## cru.oil: Building Views (Obvious Interface Language)

The `cru.oil` module provides a **functional, React-like API** for building TUI nodes. Components are functions that return node trees, and composition happens via function calls.

### Basic Usage (Lua)

```lua
local oil = cru.oil

-- Text with styling
local heading = oil.text("Tasks", { bold = true, fg = "blue" })

-- Layout containers (col = vertical, row = horizontal)
local view = oil.col({ gap = 1, padding = 1, border = "rounded" },
    oil.text("Header", { bold = true }),
    oil.row(
        oil.badge("OK", { fg = "green" }),
        oil.spacer(),
        oil.text("Status")
    ),
    oil.divider()
)

-- Lists
local bullets = oil.bullet_list({ "First item", "Second item" })

-- Progress indicators
local progress = oil.progress(0.75)
local loading = oil.spinner("Loading...")
```

### Control Flow

Use control flow functions for conditional and iterative rendering:

```lua
-- Conditional rendering
oil.when(is_loading, oil.spinner("Loading..."))

-- Conditional with else branch
oil.if_else(is_online,
    oil.text("Online", { fg = "green" }),
    oil.text("Offline", { fg = "red" })
)

-- Iterate over items
oil.each(items, function(item)
    return oil.text(item.name)
end)
```

### Reusable Components

Create reusable components with the `component` factory:

```lua
-- Create a Card component with default props
local Card = oil.component(oil.col, { padding = 2, border = "rounded" })

-- Use with additional props (merged with defaults)
local view = Card({ gap = 1 },
    oil.text("Card Title", { bold = true }),
    oil.text("Card body content")
)
```

Or define components as regular functions:

```lua
local function StatusBar(props)
    return oil.row({ justify = "space_between" },
        oil.text(props.title, { bold = true }),
        oil.badge(props.status, { fg = props.color })
    )
end

-- Usage
StatusBar({ title = "Dashboard", status = "OK", color = "green" })
```

### Available Components

| Function | Description |
|----------|-------------|
| `oil.text(content, style?)` | Styled text |
| `oil.col(props?, children...)` | Vertical flex container |
| `oil.row(props?, children...)` | Horizontal flex container |
| `oil.fragment(children...)` | Invisible wrapper |
| `oil.spacer()` | Flexible space filler |
| `oil.divider(char?, width?)` | Horizontal line |
| `oil.hr()` | Full-width horizontal rule |
| `oil.badge(label, style?)` | Colored badge |
| `oil.spinner(label?)` | Loading spinner |
| `oil.progress(value, width?)` | Progress bar (value 0-1) |
| `oil.input(opts)` | Text input field |
| `oil.popup(items, selected?, max?)` | Popup menu |
| `oil.bullet_list(items)` | Bulleted list |
| `oil.numbered_list(items)` | Numbered list |
| `oil.kv(key, value)` | Key-value pair row |
| `oil.scrollback(key, children...)` | Scrollable container |

### Control Flow Functions

| Function | Description |
|----------|-------------|
| `oil.when(cond, node)` | Show node if condition is truthy |
| `oil.if_else(cond, t, f)` | Show t if true, f if false (alias: `either`) |
| `oil.each(items, fn)` | Map items to nodes |
| `oil.component(base, defaults)` | Create component with default props |

### Style Options

```lua
local style = {
    fg = "red",        -- Foreground: red, green, blue, yellow, etc. or "#hex"
    bg = "blue",       -- Background color
    bold = true,       -- Bold text
    dim = true,        -- Dimmed text
    italic = true,     -- Italic text
    underline = true,  -- Underlined text
}
```

### Container Options

```lua
local opts = {
    gap = 1,                -- Space between children
    padding = 2,            -- Inner padding (all sides)
    margin = 1,             -- Outer margin (all sides)
    border = "rounded",     -- single, double, rounded, heavy
    justify = "center",     -- start, end, center, space_between, space_around, space_evenly
    align = "stretch",      -- start, end, center, stretch
}
```

### Markup Syntax

For template-driven UI, parse XML-like markup:

```lua
local view = oil.markup([[
    <col border="rounded" gap="1">
        <text bold="true">Header</text>
        <divider />
        <text>Content here</text>
    </col>
]])
```

## Fennel Support

The `lib/oil.fnl` module provides idiomatic Fennel wrappers:

```fennel
;; Load the oil module
(local oil (require :oil))

;; Define a reusable component
(oil.defui status-bar [{: title : status : color}]
  (oil.row {:justify :space-between}
    (oil.text title {:bold true})
    (oil.badge status {:fg color})))

;; Build a view
(oil.col {:gap 1 :padding 1 :border :rounded}
  (status-bar {:title "Dashboard" :status "OK" :color :green})
  (oil.text "Welcome back!")
  (oil.when loading (oil.spinner "Loading..."))
  (oil.map-each items (fn [item]
    (oil.text item.name))))
```

### Fennel-Specific Features

| Macro/Function | Description |
|----------------|-------------|
| `(oil.defui name [props] body)` | Define a component function |
| `(oil.cond-ui ...)` | Multi-branch conditional |
| `(oil.when cond node)` | Conditional rendering |
| `(oil.map-each items fn)` | Iterate items (named to avoid shadowing) |

### Multi-Branch Conditional

```fennel
(oil.cond-ui
  loading (oil.spinner "Loading...")
  error   (oil.text error {:fg :red})
  :else   (oil.text "Ready" {:fg :green}))
```

## See Also

- [[Help/Lua/Language Basics]] - Lua reference
- [[Help/Lua/Configuration]] - Lua configuration
- [[Help/TUI/Component Architecture]] - UI internals
- [[Help/Extending/Creating Plugins]] - Plugin development
- [[Help/Extending/Plugin Manifest]] - Plugin manifest format
