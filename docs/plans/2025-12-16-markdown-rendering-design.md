# Markdown Rendering for TUI

## Overview

Add markdown rendering with syntax highlighting to the TUI chat interface. Uses termimad for markdown structure and syntect for code block highlighting.

## Architecture

```
Assistant response (raw markdown)
        ↓
MarkdownRenderer::render(&str) -> String
        ↓
    termimad (structure: bold, italic, lists, blockquotes)
        ↓
    syntect (code blocks with language detection)
        ↓
Terminal output (ANSI codes)
```

## Dependencies

```toml
# crates/crucible-cli/Cargo.toml
termimad = "0.30"
syntect = { version = "5.2", default-features = false, features = ["default-syntaxes", "default-themes", "regex-onig"] }
```

## Components

### MarkdownRenderer

Location: `crates/crucible-cli/src/tui/markdown.rs`

```rust
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    dark_theme: Theme,
    light_theme: Theme,
    skin_dark: MadSkin,
    skin_light: MadSkin,
    is_dark: bool,
}

impl MarkdownRenderer {
    /// Create renderer with auto-detected theme
    pub fn new() -> Self;

    /// Render markdown to ANSI-styled string
    pub fn render(&self, markdown: &str) -> String;

    /// Render a code block with syntax highlighting
    fn render_code_block(&self, code: &str, lang: &str) -> String;

    /// Detect terminal background (dark vs light)
    fn detect_terminal_background() -> bool;
}
```

### Theme Detection

Priority order:
1. `COLORFGBG` env var (format: "fg;bg", bg > 6 = light)
2. `TERM_BACKGROUND` env var ("dark" | "light")
3. Default to dark

Themes:
- Dark: `base16-ocean.dark`
- Light: `base16-ocean.light`

### Key Behaviors

- Lazy-load syntax definitions once at construction (~2MB)
- Fall back to plain text for unknown languages
- Preserve newlines for terminal scrollback compatibility
- Handle inline code with termimad styling
- Handle fenced code blocks with syntect

## Integration Points

### 1. TuiRunner (tui/runner.rs)

Add renderer field and print method:

```rust
pub struct TuiRunner {
    state: TuiState,
    renderer: MarkdownRenderer,  // NEW
    width: u16,
    height: u16,
}

impl TuiRunner {
    fn print_assistant_response(&mut self, content: &str) -> Result<()> {
        let rendered = self.renderer.render(content);
        // Print above widget area
        writeln!(stdout, "\x1b[1mAssistant:\x1b[0m")?;
        write!(stdout, "{}", rendered)?;
    }
}
```

### 2. Response Finalization

When `AgentResponded` event is received, call `print_assistant_response()` with the full content.

### 3. One-shot Mode (commands/chat.rs)

Use same renderer for consistency when printing one-shot query responses.

## Files Changed

| File | Change |
|------|--------|
| `Cargo.toml` | Add termimad, syntect deps |
| `tui/mod.rs` | Export markdown module |
| `tui/markdown.rs` | NEW: MarkdownRenderer (~150-200 lines) |
| `tui/runner.rs` | Add renderer field, print_assistant_response() |
| `commands/chat.rs` | Use renderer for one-shot output |

## Future Enhancements

- Config option: `[tui] theme = "auto" | "dark" | "light"`
- Custom themes from file
- Configurable code block languages to highlight
