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
local popup = require("crucible.popup")

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
local ui = require("crucible.ui")

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
local ui = require("crucible.ui")

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
local ui = require("crucible.ui")

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
    local result = crucible.show_panel(panel)

    if result.cancelled then
        return { message = "Cancelled" }
    else
        local selected = items[result.selected[1]].label
        return { message = "You chose: " .. selected }
    end
end
```

## Fennel Support

The same API is available in Fennel:

```fennel
;; database_selector.fnl
(local ui (require "crucible.ui"))

(fn choose-database []
  (let [items [(ui.panel_item "PostgreSQL" "Full-featured RDBMS")
               (ui.panel_item "SQLite" "Embedded, single-file")]
        panel (ui.panel "Select database" items)
        result (crucible.show_panel panel)]
    (if result.cancelled
        {:message "Cancelled"}
        {:message (.. "You chose: " (. (. items (. result.selected 1)) :label))})))

{:choose_database choose-database}
```

## See Also

- [[Help/Lua/Language Basics]] - Lua reference
- [[Help/Lua/Configuration]] - Lua configuration
- [[Help/TUI/Component Architecture]] - UI internals
- [[Help/Extending/Creating Plugins]] - Plugin development
