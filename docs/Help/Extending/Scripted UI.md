---
description: Create interactive popups and panels from scripts
status: implemented
tags:
  - scripting
  - ui
  - popup
  - panel
  - rune
  - steel
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

## Rune

### PopupRequest

```rune
use crucible::popup::{entry, request, request_with_other};

// Create entries
let entries = [
    entry("Daily Note", Some("Today's journal")),
    entry("Todo List", None),
];

// Basic popup
let popup = request("Select a note", entries);

// Popup that allows free-text input
let search = request_with_other("Search or select", entries);
```

### InteractivePanel

```rune
use crucible::panel::{item, panel, confirm, select, multi_select};

// Create panel items
let items = [
    item("PostgreSQL", Some("Full-featured RDBMS")),
    item("SQLite", Some("Embedded, single-file")),
];

// Basic panel
let db_panel = panel("Select database", items);

// Convenience functions
let confirmed = confirm("Delete this file?");           // Yes/No
let choice = select("Pick one", ["A", "B", "C"]);       // Single select
let choices = multi_select("Pick many", ["X", "Y"]);    // Multi-select
```

### Panel Hints

Control panel behavior with hints:

```rune
use crucible::panel::{Panel, PanelHints};

let hints = PanelHints::new()
    .filterable()      // Enable search/filter
    .multi_select()    // Allow multiple selections
    .allow_other();    // Allow free-text input

let panel = Panel::new("Choose options")
    .items(items)
    .hints(hints);
```

## Steel

### PopupRequest

```scheme
;; Create popup entries
(define entries
  (list
    (popup-entry "Daily Note" "Today's journal")
    (popup-entry "Todo List" #f)))

;; Basic popup
(define popup (popup-request "Select a note" entries))

;; Allow free-text input
(define search (popup-request-with-other "Search or select" entries))
```

### InteractivePanel

```scheme
;; Create panel items
(define items
  (list
    (panel-item "PostgreSQL" "Full-featured RDBMS")
    (panel-item "SQLite" "Embedded, single-file")))

;; Basic panel
(define db-panel (panel "Select database" items))

;; Convenience functions
(define confirmed (confirm "Delete this file?"))
(define choice (select "Pick one" (list "A" "B" "C")))
(define choices (multi-select "Pick many" (list "X" "Y")))
```

### Panel Hints

```scheme
;; Create hints for panel behavior
(define hints (panel-hints-multi-select))  ; or panel-hints-filterable

;; Panel with custom hints
(define panel (panel-with-hints "Choose" items hints))
```

## Lua

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

```lua
local ui = require("crucible.ui")

-- Create hints
local hints = ui.panel_hints()
    :filterable()
    :multi_select()
    :allow_other()

-- Panel with hints
local panel = ui.panel_with_hints("Choose", items, hints)
```

## Handling Results

### PopupResponse

When user selects from a popup:

```rune
// Rune
match response {
    PopupResponse::Selected { index, entry } => {
        // User selected an entry
    }
    PopupResponse::Other { text } => {
        // User entered free text
    }
    PopupResponse::None => {
        // User dismissed the popup
    }
}
```

```scheme
;; Steel
(cond
  [(hash-ref response 'selected_index)
   => (lambda (idx) (handle-selection idx))]
  [(hash-ref response 'other)
   => (lambda (text) (handle-text text))]
  [else (handle-dismiss)])
```

```lua
-- Lua
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

```rune
// Rune
if result.cancelled {
    // User cancelled
} else if let Some(other) = result.other {
    // Free-text input
} else {
    // result.selected contains indices
    for idx in result.selected {
        // Process selection
    }
}
```

```scheme
;; Steel
(cond
  [(hash-ref result 'cancelled) (handle-cancel)]
  [(hash-ref result 'other) => handle-text]
  [else (for-each handle-item (hash-ref result 'selected))])
```

```lua
-- Lua
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

```rune
// Rune - database_selector.rn
use crucible::panel::{item, panel};

#[tool(
    name = "choose_database",
    description = "Select a database type for your project"
)]
pub async fn choose_database() -> Result {
    let items = [
        item("PostgreSQL", Some("Full-featured, ACID-compliant RDBMS")),
        item("SQLite", Some("Embedded, zero-configuration")),
        item("SurrealDB", Some("Multi-model with graph queries")),
    ];

    let panel = panel("Select database", items);

    // Display panel and get result
    let result = crucible::show_panel(panel).await?;

    if result.cancelled {
        Ok("Cancelled")
    } else {
        let selected = items[result.selected[0]].label;
        Ok(format!("You chose: {}", selected))
    }
}
```

## See Also

- [[Help/Concepts/Scripting Languages]] - Language overview
- [[Help/Rune/Crucible API]] - Rune API reference
- [[Help/Steel/Language Basics]] - Steel reference
- [[Help/Lua/Language Basics]] - Lua reference
- [[Help/TUI/Component Architecture]] - UI internals
