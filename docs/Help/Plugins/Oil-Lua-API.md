# Oil Lua API Reference

> **Build custom TUI components with Lua**

Oil is Crucible's declarative UI library for building terminal interfaces. This API allows Lua plugins to create custom views, components, and interactive elements using a composable, React-like approach.

## Overview

The Oil Lua API provides:

- **Layout Primitives**: Build UIs with columns, rows, and flexible spacing
- **Widgets**: Spinners, progress bars, input fields, popups, and more
- **Styling**: Colors, borders, padding, and text attributes
- **Conditional Rendering**: Show/hide elements based on state
- **Component Composition**: Build reusable UI components
- **Markup Syntax**: Quick prototyping with XML-like syntax

All Oil functions are available under the `cru.oil` namespace.

## Quick Start

```lua
local oil = cru.oil

-- Simple text
local hello = oil.text("Hello, World!")

-- Styled text
local styled = oil.text("Important!", { fg = "red", bold = true })

-- Layout with column
local view = oil.col({ gap = 1 },
    oil.text("Title", { bold = true }),
    oil.text("Content goes here"),
    oil.spinner("Loading...")
)

-- Conditional rendering
local loading_view = oil.when(is_loading, oil.spinner("Please wait..."))
```

---

## API Reference

### Layout Primitives

#### `text(content, opts?)`

Display text with optional styling.

**Parameters:**
- `content` (string|number): Text content to display
- `opts` (table, optional): Styling options

**Returns:** Node

**Example:**
```lua
oil.text("Hello")
oil.text("Error!", { fg = "red", bold = true })
oil.text(42, { fg = "cyan" })
```

---

#### `col(opts_or_children...)`

Vertical layout container (column).

**Parameters:**
- First argument can be options table (if it contains layout keys like `gap`, `padding`, etc.)
- Remaining arguments are child nodes

**Options:**
- `gap` (number): Spacing between children
- `padding` (number): Internal padding
- `margin` (number): External margin
- `border` (string): Border style ("single", "double", "rounded", "heavy")
- `justify` (string): Vertical alignment ("start", "end", "center", "space_between", "space_around", "space_evenly")
- `align` (string): Horizontal alignment ("start", "end", "center", "stretch")

**Returns:** Node

**Example:**
```lua
-- Simple column
oil.col(
    oil.text("Line 1"),
    oil.text("Line 2")
)

-- Column with options
oil.col({ gap = 2, padding = 1, border = "rounded" },
    oil.text("Title"),
    oil.text("Content")
)
```

---

#### `row(opts_or_children...)`

Horizontal layout container (row).

**Parameters:** Same as `col()`

**Returns:** Node

**Example:**
```lua
-- Simple row
oil.row(
    oil.text("Left"),
    oil.spacer(),
    oil.text("Right")
)

-- Row with gap
oil.row({ gap = 2 },
    oil.text("Item 1"),
    oil.text("Item 2"),
    oil.text("Item 3")
)
```

---

#### `spacer()`

Flexible space that expands to fill available space. Useful for pushing elements apart in rows.

**Returns:** Node

**Example:**
```lua
oil.row(
    oil.text("Left"),
    oil.spacer(),  -- Pushes "Right" to the far right
    oil.text("Right")
)
```

---

#### `fragment(children...)`

Container that renders children without adding layout. Useful for grouping.

**Parameters:**
- `children...`: Child nodes

**Returns:** Node

**Example:**
```lua
oil.fragment(
    oil.text("Item 1"),
    oil.text("Item 2")
)
```

---

### Widgets

#### `spinner(label?)`

Animated loading spinner.

**Parameters:**
- `label` (string, optional): Text to display next to spinner

**Returns:** Node

**Example:**
```lua
oil.spinner()
oil.spinner("Loading data...")
```

---

#### `input(opts)`

Text input field with cursor.

**Parameters:**
- `opts` (table):
  - `value` (string): Current input value
  - `cursor` (number): Cursor position
  - `placeholder` (string, optional): Placeholder text
  - `focused` (boolean, optional): Whether input is focused (default: true)

**Returns:** Node

**Example:**
```lua
oil.input({
    value = "Hello",
    cursor = 5,
    placeholder = "Type here...",
    focused = true
})
```

---

#### `popup(items, selected?, max_visible?)`

Popup menu with selectable items.

**Parameters:**
- `items` (table): Array of items (strings or tables with `label`, `desc`, `kind`)
- `selected` (number, optional): Index of selected item (default: 0)
- `max_visible` (number, optional): Maximum visible items (default: 10)

**Returns:** Node

**Example:**
```lua
-- Simple popup
oil.popup({"Option 1", "Option 2", "Option 3"}, 0, 5)

-- Popup with descriptions
oil.popup({
    {label = "Save", desc = "Save current file", kind = "action"},
    {label = "Load", desc = "Load from disk", kind = "action"},
    {label = "Exit", desc = "Quit application", kind = "danger"}
}, 0, 10)
```

---

#### `progress(value, width?)`

Progress bar.

**Parameters:**
- `value` (number): Progress value (0.0 to 1.0)
- `width` (number, optional): Bar width in characters (default: 20)

**Returns:** Node

**Example:**
```lua
oil.progress(0.5, 30)  -- 50% progress, 30 chars wide
oil.progress(0.75, 40) -- 75% progress, 40 chars wide
```

---

#### `badge(label, opts?)`

Small labeled badge.

**Parameters:**
- `label` (string): Badge text
- `opts` (table, optional): Styling options

**Returns:** Node

**Example:**
```lua
oil.badge("NEW")
oil.badge("ERROR", { fg = "red", bold = true })
oil.badge("v1.2.3", { fg = "cyan" })
```

---

#### `divider(char?, width?)`

Horizontal divider line.

**Parameters:**
- `char` (string, optional): Character to repeat (default: "─")
- `width` (number, optional): Line width (default: 80)

**Returns:** Node

**Example:**
```lua
oil.divider()           -- ─────────...
oil.divider("=", 40)    -- ========...
oil.divider("*", 20)    -- ********...
```

---

#### `hr()`

Horizontal rule (same as `divider()` with defaults).

**Returns:** Node

**Example:**
```lua
oil.hr()
```

---

### Lists

#### `bullet_list(items)`

Bulleted list.

**Parameters:**
- `items` (table): Array of strings

**Returns:** Node

**Example:**
```lua
oil.bullet_list({
    "First item",
    "Second item",
    "Third item"
})
```

---

#### `numbered_list(items)`

Numbered list.

**Parameters:**
- `items` (table): Array of strings

**Returns:** Node

**Example:**
```lua
oil.numbered_list({
    "Step one",
    "Step two",
    "Step three"
})
```

---

#### `kv(key, value)`

Key-value pair display.

**Parameters:**
- `key` (string): Key label
- `value` (string): Value text

**Returns:** Node

**Example:**
```lua
oil.kv("Name", "John Doe")
oil.kv("Status", "Active")
```

---

### Conditional Rendering

#### `when(condition, node)`

Render node only if condition is true.

**Parameters:**
- `condition` (boolean): Condition to check
- `node` (Node): Node to render if true

**Returns:** Node (or empty node if false)

**Example:**
```lua
oil.when(is_loading, oil.spinner("Loading..."))
oil.when(has_error, oil.text("Error occurred!", { fg = "red" }))
```

---

#### `either(condition, true_node, false_node)`

Render one of two nodes based on condition. Also available as `if_else`.

**Parameters:**
- `condition` (boolean): Condition to check
- `true_node` (Node): Node to render if true
- `false_node` (Node): Node to render if false

**Returns:** Node

**Example:**
```lua
oil.either(is_ready,
    oil.text("Ready!", { fg = "green" }),
    oil.spinner("Initializing...")
)

-- Also available as if_else
oil.if_else(has_data,
    oil.text(data),
    oil.text("No data", { fg = "yellow" })
)
```

---

### Iteration

#### `each(items, fn)`

Map over items and render each one.

**Parameters:**
- `items` (table): Array of items
- `fn` (function): Function that takes `(item, index)` and returns a Node

**Returns:** Node (fragment containing all rendered items)

**Example:**
```lua
local names = {"Alice", "Bob", "Charlie"}

oil.each(names, function(name, idx)
    return oil.text(idx .. ". " .. name)
end)
```

---

### Advanced

#### `markup(xml_string)`

Parse XML-like markup into nodes. Useful for quick prototyping.

**Parameters:**
- `xml_string` (string): XML-like markup

**Supported tags:**
- `<div>` → col
- `<p>` → text
- `<ul>` → bullet_list
- `<li>` → list item

**Attributes:**
- `gap`, `padding`, `margin`, `border`
- `fg`, `bg`, `bold`, `dim`, `italic`, `underline`

**Returns:** Node

**Example:**
```lua
oil.markup([[
    <div gap="2" padding="1" border="rounded">
        <p bold="true" fg="cyan">Title</p>
        <p>Content goes here</p>
        <ul>
            <li>Item 1</li>
            <li>Item 2</li>
        </ul>
    </div>
]])
```

---

#### `component(base_fn, defaults)`

Create a reusable component with default props.

**Parameters:**
- `base_fn` (function): Base function (like `oil.col` or `oil.row`)
- `defaults` (table): Default options

**Returns:** Function that merges user props with defaults

**Example:**
```lua
-- Create a Card component with default styling
local Card = oil.component(oil.col, {
    border = "rounded",
    padding = 1,
    gap = 1
})

-- Use it with custom props
Card({ gap = 2 },
    oil.text("Title", { bold = true }),
    oil.text("Content")
)

-- Use it with defaults only
Card(
    oil.text("Simple card")
)
```

---

#### `scrollback(key, children...)`

Scrollable content area. Preserves scroll position across renders.

**Parameters:**
- `key` (string): Unique key for this scrollback area
- `children...`: Child nodes

**Returns:** Node

**Example:**
```lua
oil.scrollback("chat-messages",
    oil.text("Message 1"),
    oil.text("Message 2"),
    oil.text("Message 3")
)
```

---

#### `decrypt(content, revealed, frame?)`

Animated decrypt/scramble effect.

**Parameters:**
- `content` (string): Text to decrypt
- `revealed` (number): Number of characters revealed
- `frame` (number, optional): Animation frame (default: 0)

**Returns:** Node

**Example:**
```lua
-- Gradually reveal text
oil.decrypt("Secret message", 6, 0)  -- "Secret█████████"
oil.decrypt("Secret message", 14, 0) -- "Secret message"
```

---

### Node Methods

All nodes support chainable methods for styling:

#### `:with_style(opts)`

Apply styling to a node.

**Example:**
```lua
oil.text("Hello"):with_style({ fg = "red", bold = true })
```

---

#### `:with_padding(n)`

Add padding to a node.

**Example:**
```lua
oil.col(oil.text("Content")):with_padding(2)
```

---

#### `:with_border(type)`

Add border to a node.

**Example:**
```lua
oil.col(oil.text("Boxed")):with_border("rounded")
```

---

#### `:with_margin(n)`

Add margin to a node.

**Example:**
```lua
oil.text("Spaced"):with_margin(1)
```

---

#### `:gap(n)`

Set gap between children.

**Example:**
```lua
oil.col(child1, child2):gap(2)
```

---

#### `:justify(mode)`

Set justify mode.

**Example:**
```lua
oil.col(child1, child2):justify("center")
```

---

#### `:align(mode)`

Set align mode.

**Example:**
```lua
oil.row(child1, child2):align("center")
```

---

## Styling Options

### Colors

**Named colors:**
- `"red"`, `"green"`, `"blue"`, `"cyan"`, `"yellow"`, `"magenta"`, `"white"`, `"black"`

**Hex colors:**
- `"#ff0000"`, `"#00ff00"`, `"#0000ff"`, etc.

**Usage:**
```lua
{ fg = "red" }           -- Foreground color
{ bg = "blue" }          -- Background color
{ fg = "#ff5500" }       -- Hex color
```

### Text Attributes

```lua
{ bold = true }          -- Bold text
{ dim = true }           -- Dimmed text
{ italic = true }        -- Italic text
{ underline = true }     -- Underlined text
```

### Layout Options

```lua
{ gap = 2 }              -- Spacing between children
{ padding = 1 }          -- Internal padding
{ margin = 1 }           -- External margin
{ border = "rounded" }   -- Border style
{ justify = "center" }   -- Justify content
{ align = "center" }     -- Align items
```

**Border styles:**
- `"single"` - Single line border
- `"double"` - Double line border
- `"rounded"` - Rounded corners
- `"heavy"` - Heavy/thick border

**Justify modes:**
- `"start"` - Align to start
- `"end"` - Align to end
- `"center"` - Center items
- `"space_between"` - Space between items
- `"space_around"` - Space around items
- `"space_evenly"` - Even spacing

**Align modes:**
- `"start"` - Align to start
- `"end"` - Align to end
- `"center"` - Center items
- `"stretch"` - Stretch to fill

---

## Common Patterns

### Message Block

```lua
local function message_block(role, content)
    local role_colors = {
        user = "green",
        assistant = "blue",
        system = "yellow"
    }
    
    local color = role_colors[role] or "white"
    
    return oil.col({ gap = 0 },
        oil.text(""),
        oil.text(string.upper(role), { fg = color, bold = true }),
        oil.col({ padding = 1, border = "single" },
            oil.text(content)
        ),
        oil.text("")
    )
end

-- Usage
message_block("user", "Hello, how are you?")
message_block("assistant", "I'm doing well, thank you!")
```

### Status Bar

```lua
local function status_bar(mode, model, context_pct)
    local mode_colors = {
        NORMAL = "green",
        PLAN = "blue",
        AUTO = "yellow"
    }
    
    return oil.row({ gap = 2 },
        oil.text(" " .. mode .. " ", {
            bg = mode_colors[mode],
            fg = "black",
            bold = true
        }),
        oil.text(model, { fg = "cyan" }),
        oil.spacer(),
        oil.text(string.format("%d%% ctx", context_pct), { fg = "yellow" })
    )
end

-- Usage
status_bar("NORMAL", "gpt-4o", 45)
```

### Tool Call Display

```lua
local function tool_call_display(name, status, result)
    local status_colors = {
        pending = "yellow",
        running = "cyan",
        complete = "green",
        error = "red"
    }
    
    return oil.col({ border = "rounded", padding = 1, gap = 1 },
        oil.row({ gap = 2 },
            oil.text("🔧", { bold = true }),
            oil.text(name, { bold = true }),
            oil.badge(status, { fg = status_colors[status] })
        ),
        oil.when(status == "running", oil.spinner("Processing...")),
        oil.when(result ~= nil,
            oil.col({ gap = 0 },
                oil.divider("─", 40),
                oil.text(result)
            )
        )
    )
end

-- Usage
tool_call_display("search", "running", nil)
tool_call_display("search", "complete", "Found 5 results")
```

### Loading State

```lua
local function loading_view(is_loading, error_msg, data)
    return oil.col({ gap = 1 },
        oil.text("Data Viewer", { bold = true }),
        oil.hr(),
        
        -- Show spinner while loading
        oil.when(is_loading,
            oil.col({ gap = 1 },
                oil.spinner("Loading..."),
                oil.text("Please wait", { fg = "yellow" })
            )
        ),
        
        -- Show error if present
        oil.when(error_msg ~= nil,
            oil.col({ border = "heavy", padding = 1 },
                oil.text("Error", { fg = "red", bold = true }),
                oil.text(error_msg, { fg = "red" })
            )
        ),
        
        -- Show data when ready
        oil.when(not is_loading and error_msg == nil and data ~= nil,
            oil.text(data)
        )
    )
end
```

### Progress Indicator

```lua
local function progress_indicator(label, current, total)
    local percentage = math.floor((current / total) * 100)
    
    return oil.col({ gap = 0 },
        oil.row({ gap = 2 },
            oil.text(label),
            oil.text(string.format("%d/%d", current, total), { fg = "cyan" })
        ),
        oil.progress(current / total, 40),
        oil.text(string.format("%d%%", percentage), { fg = "green" })
    )
end

-- Usage
progress_indicator("Processing files", 7, 10)
```

### Card Component

```lua
local function card(title, content, opts)
    opts = opts or {}
    local border_style = opts.border or "rounded"
    local padding = opts.padding or 1
    
    return oil.col({
        border = border_style,
        padding = padding,
        gap = 1
    },
        oil.text(title, { bold = true, fg = "cyan" }),
        content
    )
end

-- Usage
card("System Info", oil.col({ gap = 0 },
    oil.kv("Model", "gpt-4o"),
    oil.kv("Status", "Ready"),
    oil.kv("Uptime", "2h 34m")
))
```

---

## Best Practices

### Component Composition

Build complex UIs from simple, reusable components:

```lua
-- Define reusable components
local function InfoRow(label, value)
    return oil.row({ gap = 2 },
        oil.text(label .. ":", { fg = "cyan" }),
        oil.text(value)
    )
end

local function Card(title, children)
    return oil.col({ border = "rounded", padding = 1, gap = 1 },
        oil.text(title, { bold = true }),
        children
    )
end

-- Compose them
local view = Card("User Info",
    oil.col({ gap = 0 },
        InfoRow("Name", "Alice"),
        InfoRow("Role", "Admin"),
        InfoRow("Status", "Active")
    )
)
```

### Styling Conventions

Use semantic colors and consistent spacing:

```lua
-- Good: Semantic colors
local colors = {
    success = "green",
    error = "red",
    warning = "yellow",
    info = "blue"
}

-- Good: Consistent spacing
local SPACING = {
    tight = 0,
    normal = 1,
    loose = 2
}

oil.col({ gap = SPACING.normal },
    oil.text("Title"),
    oil.text("Content")
)
```

### Conditional Rendering

Use `when()` and `either()` for clean conditional logic:

```lua
-- Good: Clear conditional rendering
oil.when(is_loading, oil.spinner("Loading..."))

oil.either(has_data,
    oil.text(data),
    oil.text("No data available", { fg = "yellow" })
)

-- Avoid: Lua conditionals that return nil
-- Bad: if is_loading then return oil.spinner() end
```

### Performance Tips

1. **Avoid deep nesting**: Keep component trees shallow
2. **Use keys for lists**: Helps with efficient re-rendering
3. **Memoize expensive computations**: Cache results when possible
4. **Use `fragment()` sparingly**: Only when you need grouping without layout

---

## Examples

See [`examples/plugins/custom-ui.lua`](../../../examples/plugins/custom-ui.lua) for comprehensive examples including:

- Chat interface with messages and tool calls
- Dashboard with cards and statistics
- Progress tracking with multiple indicators
- Conditional rendering patterns
- Markup syntax examples
- Component composition techniques

---

## See Also

- [Plugin System Overview](./Plugins.md)
- [Lua Scripting Guide](../Concepts/Scripting%20Languages.md)
- [Example Plugins](../../../examples/plugins/)
