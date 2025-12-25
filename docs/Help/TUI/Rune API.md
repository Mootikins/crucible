---
description: Rune scripting API for TUI customization (planned)
tags:
  - tui
  - rune
  - api
  - extensibility
status: planned
---

# TUI Rune API

> **Status:** This API is planned but not yet implemented. This document serves as a design sketch for future development.

The TUI Rune API will allow scripts to customize and extend the terminal interface.

## Planned Capabilities

### Content Hooks

Modify content before rendering:

```rune
// Transform message content before display
#[hook(tui::before_render_message)]
pub fn highlight_keywords(message) {
    // Add highlighting to specific patterns
    message.content = message.content.replace(
        "TODO",
        "\x1b[33mTODO\x1b[0m"  // Yellow highlight
    );
    message
}
```

### Custom Widgets

Register custom widget renderers:

```rune
// Define a custom status indicator
#[widget("my_status")]
pub fn render_status(area, state) {
    let text = format!("Custom: {}", state.custom_field);
    Widget::text(text)
        .style(Style::bold())
        .render(area)
}
```

### Event Handlers

Intercept and handle TUI events:

```rune
// Custom keybinding
#[hook(tui::key_event)]
pub fn handle_custom_keys(event) {
    if event.key == "Ctrl+K" {
        // Custom action
        tui::show_notification("Custom action triggered");
        return EventResult::Consumed;
    }
    EventResult::Ignored
}
```

### Popup Providers

Add custom popup item sources:

```rune
// Provide items for @ popup
#[popup_provider(kind = "agent_or_file")]
pub fn provide_recent_files(query) {
    // Return recently accessed files matching query
    let files = crucible::recent_files(10);
    files
        .filter(|f| f.name.contains(query))
        .map(|f| PopupItem::file(f.path, f.modified))
        .collect()
}
```

### Dialog Customization

Create custom dialog types:

```rune
// Custom confirmation with extra options
pub fn confirm_with_options(title, options) {
    tui::show_dialog(Dialog::select(title, options))
}
```

## Planned Types

### TuiState

Read-only access to TUI state:

```rune
pub struct TuiState {
    mode: String,           // "plan", "act", "auto"
    input: String,          // Current input buffer
    scroll_offset: i64,     // Conversation scroll position
    has_popup: bool,        // Popup is visible
    has_dialog: bool,       // Dialog is visible
}
```

### Widget

Base widget builder:

```rune
pub struct Widget {
    fn text(content: String) -> Widget;
    fn block(title: String) -> Widget;
    fn list(items: Vec<String>) -> Widget;

    fn style(self, style: Style) -> Widget;
    fn render(self, area: Rect);
}
```

### Style

Text styling:

```rune
pub struct Style {
    fn new() -> Style;
    fn bold() -> Style;
    fn italic() -> Style;
    fn fg(color: Color) -> Style;
    fn bg(color: Color) -> Style;
}
```

### EventResult

Event handling result:

```rune
pub enum EventResult {
    Consumed,
    Ignored,
    Action(TuiAction),
}
```

## Integration Points

### Component Registration

Scripts can register components at startup:

```rune
// In plugin init
pub fn init() {
    tui::register_widget("my_widget", render_my_widget);
    tui::register_hook("before_render", my_hook);
}
```

### State Access

Read TUI state from any hook:

```rune
#[hook(tui::tick)]
pub fn on_tick() {
    let state = tui::state();
    if state.mode == "auto" {
        // Do something in auto mode
    }
}
```

### Notifications

Display notifications from scripts:

```rune
// Info notification
tui::notify("Operation complete", Level::Info);

// Error notification
tui::notify("Something went wrong", Level::Error);
```

## Example: Custom Theme

```rune
// plugins/custom_theme.rn

#[hook(tui::init)]
pub fn apply_theme() {
    tui::set_style("user_message", Style::new().fg(Color::Cyan));
    tui::set_style("assistant_message", Style::new().fg(Color::White));
    tui::set_style("tool_running", Style::new().fg(Color::Yellow));
    tui::set_style("tool_complete", Style::new().fg(Color::Green));
    tui::set_style("tool_error", Style::new().fg(Color::Red));
}
```

## Implementation Notes

The Rune API will be implemented in phases:

1. **Phase 1:** Read-only state access, notifications
2. **Phase 2:** Event hooks, custom keybindings
3. **Phase 3:** Content transformation hooks
4. **Phase 4:** Custom widget registration
5. **Phase 5:** Full widget API with rendering primitives

Each phase builds on [[Help/TUI/Component Architecture]] and integrates with the existing [[Help/Rune/Crucible API]].

## See Also

- [[Help/TUI/Component Architecture]] - Widget system internals
- [[Help/Rune/Crucible API]] - Core Rune API
- [[Help/Extending/Creating Plugins]] - Plugin development guide
- [[Help/Rune/Tool Definition]] - Defining custom tools
