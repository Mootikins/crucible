--[[
  Custom UI Plugin Example
  
  This example demonstrates the Oil Lua API for building custom TUI components.
  It showcases all major Oil API features including:
  
  LAYOUT PRIMITIVES:
  - oil.col() - Vertical layout container
  - oil.row() - Horizontal layout container
  - oil.spacer() - Flexible space that expands to fill available space
  - oil.text() - Text content with optional styling
  
  STYLING & THEMING:
  - fg/bg colors (named: red, green, blue, cyan, yellow, magenta, white, black)
  - Text attributes: bold, dim, italic, underline
  - Layout options: gap, padding, margin, border, justify, align
  
  CONDITIONAL RENDERING:
  - oil.when(condition, node) - Show node only if condition is true
  - oil.either(condition, true_node, false_node) - If/else rendering
  
  REUSABLE COMPONENTS:
  - oil.component(base_fn, defaults) - Create components with default props
  - Custom functions that return nodes
  
  COMMON UI PATTERNS:
  - Cards with borders and padding
  - Status indicators and badges
  - Message blocks with role indicators
  - Progress bars and indicators
  - Tool call displays
  - Status bars
  
  CONVENIENCE FUNCTIONS:
  - oil.spinner() - Loading indicator
  - oil.badge() - Small labeled badge
  - oil.progress() - Progress bar
  - oil.divider() / oil.hr() - Horizontal lines
  - oil.bullet_list() / oil.numbered_list() - Lists
  - oil.kv() - Key-value pairs
  - oil.markup() - XML-like markup syntax
  - oil.each() - Map over items
  
  USAGE:
  This is an educational example showing how to build custom UIs.
  Copy patterns from here into your own plugins.
]]

local oil = cru.oil

-- ============================================================================
-- REUSABLE COMPONENTS
-- ============================================================================

-- PATTERN: Card component with border and padding
-- Demonstrates: oil.col(), border styling, padding, nested content
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

-- PATTERN: Status badge with semantic colors
-- Demonstrates: oil.badge(), color mapping, text styling
local function status_badge(status)
    local colors = {
        success = "green",
        error = "red",
        warning = "yellow",
        info = "blue",
        pending = "cyan"
    }
    
    local color = colors[status] or "white"
    return oil.badge(string.upper(status), { fg = color, bold = true })
end

-- PATTERN: Key-value pair display
-- Demonstrates: oil.row(), gap spacing, label/value styling
local function info_row(label, value)
    return oil.row({ gap = 2 },
        oil.text(label .. ":", { fg = "cyan" }),
        oil.text(value)
    )
end

-- PATTERN: Message block with role indicator
-- Demonstrates: oil.col(), oil.row(), conditional styling, borders
local function message_block(role, content)
    local role_colors = {
        user = "green",
        assistant = "blue",
        system = "yellow"
    }
    
    local role_icons = {
        user = "▶",
        assistant = "◀",
        system = "●"
    }
    
    local color = role_colors[role] or "white"
    local icon = role_icons[role] or "•"
    
    return oil.col({ gap = 0 },
        oil.text(""),
        oil.row({ gap = 1 },
            oil.text(icon, { fg = color, bold = true }),
            oil.text(string.upper(role), { fg = color, bold = true })
        ),
        oil.col({ padding = 1, border = "single" },
            oil.text(content)
        ),
        oil.text("")
    )
end

-- PATTERN: Progress indicator with label and percentage
-- Demonstrates: oil.progress(), oil.row(), formatting, color coding
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

-- PATTERN: Tool call display with status and result
-- Demonstrates: oil.when() for conditional rendering, oil.spinner(), nested layouts
local function tool_call_display(name, status, result)
    local status_colors = {
        pending = "yellow",
        running = "cyan",
        complete = "green",
        error = "red"
    }
    
    local color = status_colors[status] or "white"
    
    return oil.col({ border = "rounded", padding = 1, gap = 1 },
        oil.row({ gap = 2 },
            oil.text("🔧", { bold = true }),
            oil.text(name, { bold = true }),
            status_badge(status)
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

-- PATTERN: Status bar with mode, model, and context usage
-- Demonstrates: oil.row(), oil.spacer() for right-alignment, background colors
local function status_bar(mode, model, context_pct)
    local mode_colors = {
        NORMAL = "green",
        PLAN = "blue",
        AUTO = "yellow"
    }
    
    local mode_color = mode_colors[mode] or "white"
    
    return oil.row({ gap = 2 },
        oil.text(" " .. mode .. " ", { bg = mode_color, fg = "black", bold = true }),
        oil.text(model, { fg = "cyan" }),
        oil.spacer(),
        oil.text(string.format("%d%% ctx", context_pct), { fg = "yellow" })
    )
end

-- ============================================================================
-- EXAMPLE VIEWS
-- ============================================================================

-- VIEW 1: Simple chat interface
-- Demonstrates: message_block(), tool_call_display(), status_bar(), layout
local function chat_view()
    return oil.col({ gap = 1 },
        message_block("user", "What's the weather like today?"),
        message_block("assistant", "I'll check the weather for you."),
        tool_call_display("get_weather", "running", nil),
        status_bar("NORMAL", "gpt-4o", 45)
    )
end

-- VIEW 2: Dashboard with cards
-- Demonstrates: card(), info_row(), oil.row() for side-by-side layout, oil.bullet_list()
local function dashboard_view()
    return oil.col({ gap = 2 },
        oil.text("Dashboard", { bold = true, fg = "cyan" }),
        oil.divider("═", 60),
        
        oil.row({ gap = 2 },
            card("System Info", oil.col({ gap = 0 },
                info_row("Model", "gpt-4o-mini"),
                info_row("Status", "Ready"),
                info_row("Uptime", "2h 34m")
            )),
            
            card("Statistics", oil.col({ gap = 0 },
                info_row("Messages", "42"),
                info_row("Tools", "12"),
                info_row("Tokens", "15.2k")
            ))
        ),
        
        card("Recent Activity", oil.bullet_list({
            "Searched knowledge base",
            "Generated response",
            "Updated context"
        }))
    )
end

-- VIEW 3: Progress tracking
-- Demonstrates: progress_indicator(), status_badge(), oil.hr()
local function progress_view()
    return oil.col({ gap = 2 },
        oil.text("Task Progress", { bold = true, fg = "cyan" }),
        oil.hr(),
        
        progress_indicator("Processing files", 7, 10),
        progress_indicator("Generating embeddings", 42, 100),
        progress_indicator("Indexing", 100, 100),
        
        oil.row({ gap = 2 },
            status_badge("success"),
            oil.text("All tasks complete!")
        )
    )
end

-- VIEW 4: Conditional rendering with multiple states
-- Demonstrates: oil.when() for conditional visibility, state-based UI
local function conditional_view(loading, error_msg, data)
    return oil.col({ gap = 1 },
        oil.text("Data Viewer", { bold = true }),
        oil.hr(),
        
        oil.when(loading, oil.col({ gap = 1 },
            oil.spinner("Loading data..."),
            oil.text("Please wait...", { fg = "yellow" })
        )),
        
        oil.when(error_msg ~= nil, 
            card("Error", oil.text(error_msg, { fg = "red" }), { border = "heavy" })
        ),
        
        oil.when(not loading and error_msg == nil and data ~= nil,
            card("Data", oil.text(data))
        )
    )
end

-- VIEW 5: Using markup for quick prototyping
-- Demonstrates: oil.markup() for XML-like syntax, alternative to function calls
local function markup_view()
    return oil.markup([[
        <div gap="2">
            <p bold="true" fg="cyan">Markup Example</p>
            <hr/>
            <div border="rounded" padding="1">
                <p>You can use XML-like markup for quick prototyping!</p>
                <p fg="green">It supports styling attributes.</p>
            </div>
            <div gap="1">
                <p bold="true">Features:</p>
                <ul>
                    <li>Nested layouts</li>
                    <li>Style attributes</li>
                    <li>Quick iteration</li>
                </ul>
            </div>
        </div>
    ]])
end

-- VIEW 6: Component composition with reusable patterns
-- Demonstrates: oil.component() for creating reusable components with defaults
local function composition_view()
    local InfoCard = oil.component(oil.col, {
        border = "rounded",
        padding = 1,
        gap = 1
    })
    
    return oil.col({ gap = 2 },
        oil.text("Component Composition", { bold = true, fg = "cyan" }),
        oil.hr(),
        
        InfoCard({ gap = 2 },
            oil.text("Custom Card", { bold = true }),
            oil.text("This uses the InfoCard component with custom gap."),
            status_badge("success")
        ),
        
        InfoCard(
            oil.text("Another Card", { bold = true }),
            oil.text("Same styling, different content!"),
            oil.progress(0.75, 30)
        )
    )
end

-- VIEW 7: Advanced layout patterns
-- Demonstrates: oil.spacer() for flexible spacing, oil.each() for iteration, oil.kv()
local function advanced_layout_view()
    local items = { "Item 1", "Item 2", "Item 3" }
    
    return oil.col({ gap = 2, padding = 1 },
        oil.text("Advanced Layout Patterns", { bold = true, fg = "cyan" }),
        oil.hr(),
        
        oil.text("Using spacer() for alignment:", { bold = true }),
        oil.row({ gap = 1 },
            oil.text("Left"),
            oil.spacer(),
            oil.text("Right")
        ),
        
        oil.text("Using each() for iteration:", { bold = true }),
        oil.col({ gap = 1 },
            oil.each(items, function(item)
                return oil.kv(item, "value")
            end)
        ),
        
        oil.text("Using numbered_list():", { bold = true }),
        oil.numbered_list(items),
        
        oil.text("Border styles:", { bold = true }),
        oil.row({ gap = 1 },
            card("Single", oil.text("single"), { border = "single" }),
            card("Rounded", oil.text("rounded"), { border = "rounded" }),
            card("Heavy", oil.text("heavy"), { border = "heavy" })
        )
    )
end

-- VIEW 8: Either/if-else conditional rendering
-- Demonstrates: oil.either() for if/else patterns, alternative to multiple when()
local function either_example_view()
    local is_authenticated = true
    
    return oil.col({ gap = 2, padding = 1 },
        oil.text("Either/If-Else Example", { bold = true, fg = "cyan" }),
        oil.hr(),
        
        oil.either(is_authenticated,
            oil.col({ gap = 1 },
                oil.text("Welcome back!", { fg = "green", bold = true }),
                oil.row({ gap = 2 },
                    oil.text("User: john@example.com"),
                    oil.spacer(),
                    status_badge("success")
                )
            ),
            oil.col({ gap = 1 },
                oil.text("Please log in", { fg = "yellow", bold = true }),
                oil.text("Enter your credentials to continue")
            )
        )
    )
end

-- ============================================================================
-- PLUGIN EXPORTS
-- ============================================================================

return {
    -- Example views demonstrating different Oil API patterns
    views = {
        chat = chat_view,
        dashboard = dashboard_view,
        progress = progress_view,
        conditional = conditional_view,
        markup = markup_view,
        composition = composition_view,
        advanced_layout = advanced_layout_view,
        either_example = either_example_view,
    },
    
    -- Reusable component functions
    components = {
        card = card,
        status_badge = status_badge,
        info_row = info_row,
        message_block = message_block,
        progress_indicator = progress_indicator,
        tool_call_display = tool_call_display,
        status_bar = status_bar,
    }
}
