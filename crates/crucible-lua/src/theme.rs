//! Theme configuration types and Lua loading for the TUI theming system.
//!
//! These types define the full theme schema: colors, decorations, icons, layout,
//! and spinner style. `ThemeConfig::default_dark()` produces the built-in
//! dark theme values.
//!
//! [`load_theme_from_lua`] parses a Lua table string into a `ThemeConfig`,
//! merging partial overrides onto the dark defaults.

use crucible_oil::style::{AdaptiveColor, Color};
use mlua::{FromLua, Lua, Result as LuaResult, Table, Value};
use tracing::warn;

// ─────────────────────────────────────────────────────────────────────────────
// ThemeColors — semantic color slots using AdaptiveColor
// ─────────────────────────────────────────────────────────────────────────────

/// Semantic color palette for the TUI.
///
/// Each field is an [`AdaptiveColor`] that resolves to a concrete [`Color`]
/// based on terminal background detection (dark vs light).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeColors {
    // ── Core ─────────────────────────────────────────────────────────────
    /// Primary accent color (links, highlights, prompt)
    pub primary: AdaptiveColor,
    /// Secondary accent color (tool calls, metadata)
    pub secondary: AdaptiveColor,
    /// Main background color (input areas, panels)
    pub background: AdaptiveColor,
    /// Panel/block background (code blocks, thinking blocks)
    pub background_panel: AdaptiveColor,
    /// Command mode input background (amber tint)
    pub command_bg: AdaptiveColor,
    /// Shell mode input background (red tint)
    pub shell_bg: AdaptiveColor,
    /// Primary text color
    pub text: AdaptiveColor,
    /// Muted/secondary text color
    pub text_muted: AdaptiveColor,
    /// Dimmed text (timestamps, metadata — less prominent than muted)
    pub text_dim: AdaptiveColor,
    /// Emphasized text color (accents, highlights)
    pub text_emphasized: AdaptiveColor,

    // ── Semantic ─────────────────────────────────────────────────────────
    /// Error indicator
    pub error: AdaptiveColor,
    /// Warning indicator
    pub warning: AdaptiveColor,
    /// Success indicator
    pub success: AdaptiveColor,
    /// Info indicator
    pub info: AdaptiveColor,

    // ── Borders ──────────────────────────────────────────────────────────
    /// Default border color
    pub border: AdaptiveColor,
    /// Focused/active border color
    pub border_focused: AdaptiveColor,
    /// Dimmed border color
    pub border_dim: AdaptiveColor,

    // ── Chat roles ───────────────────────────────────────────────────────
    /// User message indicator color
    pub user_message: AdaptiveColor,
    /// Assistant message color
    pub assistant_message: AdaptiveColor,
    /// System message color
    pub system_message: AdaptiveColor,

    // ── Modes ────────────────────────────────────────────────────────────
    /// Normal mode badge background
    pub mode_normal: AdaptiveColor,
    /// Insert mode badge background
    pub mode_insert: AdaptiveColor,
    /// Plan mode badge background
    pub mode_plan: AdaptiveColor,
    /// Auto mode badge background
    pub mode_auto: AdaptiveColor,

    // ── Diff ─────────────────────────────────────────────────────────────
    /// Diff added line foreground
    pub diff_added: AdaptiveColor,
    /// Diff removed line foreground
    pub diff_removed: AdaptiveColor,
    /// Diff added line background tint
    pub diff_added_bg: AdaptiveColor,
    /// Diff removed line background tint
    pub diff_removed_bg: AdaptiveColor,
    /// Diff context line color
    pub diff_context: AdaptiveColor,

    // ── Overlay ──────────────────────────────────────────────────────────
    /// Popup/overlay background
    pub popup_bg: AdaptiveColor,
    /// Popup selected item background
    pub popup_selected_bg: AdaptiveColor,
    /// Toast notification background
    pub toast_bg: AdaptiveColor,
    /// Overlay/popup primary text color
    pub overlay_text: AdaptiveColor,
    /// Overlay bright text (action details, pure white)
    pub overlay_bright: AdaptiveColor,

    // ── Markdown rendering ────────────────────────────────────────────
    /// Inline code foreground
    pub code_inline: AdaptiveColor,
    /// Code block fallback (unsupported language) foreground
    pub code_fallback: AdaptiveColor,
    /// Fence marker (``` delimiters) foreground
    pub fence_marker: AdaptiveColor,
    /// Bullet prefix foreground
    pub bullet_prefix: AdaptiveColor,
    /// Blockquote prefix (│) foreground
    pub blockquote_prefix: AdaptiveColor,
    /// Blockquote text foreground
    pub blockquote_text: AdaptiveColor,
    /// Link foreground
    pub link: AdaptiveColor,
    /// Heading level 1 foreground
    pub heading_1: AdaptiveColor,
    /// Heading level 2 foreground
    pub heading_2: AdaptiveColor,
    /// Heading level 3 foreground
    pub heading_3: AdaptiveColor,
}

// ─────────────────────────────────────────────────────────────────────────────
// ThemeDecorations — border style, indicator chars, icons
// ─────────────────────────────────────────────────────────────────────────────

/// Visual decoration tokens: borders, indicators, and icon characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeDecorations {
    /// Border drawing style for panels and popups
    pub border_style: BorderStyle,
    /// Left-edge indicator for user messages
    pub message_user_indicator: String,
    /// Left-edge indicator for assistant messages
    pub message_assistant_indicator: String,
    /// Icon for pending tool calls
    pub tool_pending_icon: String,
    /// Icon for successful tool calls
    pub tool_success_icon: String,
    /// Icon for failed tool calls
    pub tool_error_icon: String,
    /// Bullet character for lists
    pub bullet_char: String,
    /// Horizontal divider character
    pub divider_char: String,
    /// Checkmark character
    pub check_char: String,
    /// Error/cross character
    pub error_char: String,
    /// Vertical separator character
    pub separator_char: String,
    /// Half-block top character (for gradient effects)
    pub half_block_top: char,
    /// Half-block bottom character (for gradient effects)
    pub half_block_bottom: char,
}

// ─────────────────────────────────────────────────────────────────────────────
// ThemeIcons — semantic icon characters
// ─────────────────────────────────────────────────────────────────────────────

/// Semantic icon characters used across the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeIcons {
    /// Checkmark icon (success, completion)
    pub check: String,
    /// Error icon (failure, rejection)
    pub error: String,
    /// Warning icon (caution)
    pub warning: String,
    /// Info icon (informational)
    pub info: String,
    /// Loading/spinner label
    pub loading: String,
    /// Right arrow (navigation, flow)
    pub arrow_right: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// SpinnerStyle — spinner animation variants
// ─────────────────────────────────────────────────────────────────────────────

/// Spinner animation style for loading indicators.
///
/// Each variant defines a set of animation frames cycled during async operations.
/// Use [`ThemeSpinnerStyle::frames()`] to get the frame characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeSpinnerStyle {
    /// Braille dots: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
    #[default]
    Braille,
    /// Braille minidots: ⠋ ⠙ ⠚ ⠞ ⠖ ⠦ ⠴ ⠲ ⠳ ⠓
    BrailleMinidot,
    /// ASCII-safe: - \ | /
    Ascii,
    /// Quarter-circle pulse: ◐ ◓ ◑ ◒
    Pulse,
    /// No spinner animation
    None,
}

impl ThemeSpinnerStyle {
    /// Get the frame characters for this spinner style.
    ///
    /// Returns an empty slice for [`ThemeSpinnerStyle::None`].
    pub fn frames(&self) -> &'static [char] {
        match self {
            ThemeSpinnerStyle::Braille => &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
            ThemeSpinnerStyle::BrailleMinidot => {
                &['⠋', '⠙', '⠚', '⠞', '⠖', '⠦', '⠴', '⠲', '⠳', '⠓']
            }
            ThemeSpinnerStyle::Ascii => &['-', '\\', '|', '/'],
            ThemeSpinnerStyle::Pulse => &['◐', '◓', '◑', '◒'],
            ThemeSpinnerStyle::None => &[],
        }
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// BorderStyle — theme-level border style selection
// ─────────────────────────────────────────────────────────────────────────────

/// Border drawing style for panels, popups, and input areas.
///
/// This is a theme-level abstraction over `crucible_oil::style::Border`.
/// Maps to oil's `Border` enum for actual character rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    /// Rounded corners: ╭ ╮ ╰ ╯
    #[default]
    Rounded,
    /// Sharp corners: ┌ ┐ └ ┘
    Sharp,
    /// Double-line borders: ╔ ╗ ╚ ╝
    Double,
    /// Heavy/thick borders: ┏ ┓ ┗ ┛
    Thick,
    /// ASCII-only borders: + - |
    Ascii,
    /// No visible borders
    Hidden,
}


// ─────────────────────────────────────────────────────────────────────────────
// StatusBarPosition — layout option for status bar placement
// ─────────────────────────────────────────────────────────────────────────────

/// Position of the status bar in the TUI layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarPosition {
    /// Status bar at the top of the screen
    Top,
    /// Status bar at the bottom of the screen
    #[default]
    Bottom,
    /// Status bar hidden
    Hidden,
}


// ─────────────────────────────────────────────────────────────────────────────
// ThemeLayout — layout and spacing preferences
// ─────────────────────────────────────────────────────────────────────────────

/// Layout and spacing preferences for the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeLayout {
    /// Where to display the status bar
    pub status_bar_position: StatusBarPosition,
    /// Vertical spacing (blank lines) between chat messages
    pub message_spacing: u16,
    /// Top/bottom margin around code blocks
    pub code_block_margin: u16,
    /// Maximum visible lines for the input field
    pub input_max_lines: u16,
}

impl Default for ThemeLayout {
    fn default() -> Self {
        Self {
            status_bar_position: StatusBarPosition::default(),
            message_spacing: 1,
            code_block_margin: 0,
            input_max_lines: 6,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ThemeConfig — top-level theme configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Complete theme configuration for the TUI.
///
/// Bundles colors, decorations, icons, spinner style, and layout into a single
/// struct. Use [`ThemeConfig::default_dark()`] for the built-in dark theme that
/// matches the built-in dark theme.
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    /// Human-readable theme name
    pub name: String,
    /// Whether this theme targets dark terminal backgrounds
    pub is_dark: bool,
    /// Semantic color palette
    pub colors: ThemeColors,
    /// Visual decoration characters
    pub decorations: ThemeDecorations,
    /// Semantic icon characters
    pub icons: ThemeIcons,
    /// Spinner animation style
    pub spinner: ThemeSpinnerStyle,
    /// Layout and spacing preferences
    pub layout: ThemeLayout,
}

impl ThemeConfig {
    /// Construct the default dark theme.
    ///
    /// Color values define the canonical dark theme. Each color is wrapped
    /// in `AdaptiveColor::from_single()` so dark and light variants are the
    /// same (v1 behavior).
    pub fn default_dark() -> Self {
        use Color::*;

        Self {
            name: "crucible-dark".to_string(),
            is_dark: true,
            colors: ThemeColors {
                // Core
                primary: AdaptiveColor::from_single(Cyan), // text_accent / prompt
                secondary: AdaptiveColor::from_single(Magenta), // role_tool
                background: AdaptiveColor::from_single(Rgb(40, 44, 52)), // input_bg
                background_panel: AdaptiveColor::from_single(Rgb(35, 39, 47)), // code_bg
                command_bg: AdaptiveColor::from_single(Rgb(60, 50, 20)), // command mode bg
                shell_bg: AdaptiveColor::from_single(Rgb(60, 30, 30)), // shell mode bg
                text: AdaptiveColor::from_single(White),   // text_primary
                text_muted: AdaptiveColor::from_single(DarkGray), // text_muted
                text_dim: AdaptiveColor::from_single(Gray),           // text_dim
                text_emphasized: AdaptiveColor::from_single(Cyan), // text_accent

                // Semantic
                error: AdaptiveColor::from_single(Rgb(247, 118, 142)),
                warning: AdaptiveColor::from_single(Rgb(224, 175, 104)),
                success: AdaptiveColor::from_single(Rgb(158, 206, 106)),
                info: AdaptiveColor::from_single(Rgb(0, 206, 209)),

                // Borders
                border: AdaptiveColor::from_single(Rgb(40, 44, 52)), // border (= input_bg)
                border_focused: AdaptiveColor::from_single(Cyan),    // selected
                border_dim: AdaptiveColor::from_single(Gray),        // text_dim

                // Chat roles
                user_message: AdaptiveColor::from_single(Green), // role_user
                assistant_message: AdaptiveColor::from_single(Cyan), // role_assistant
                system_message: AdaptiveColor::from_single(Yellow), // role_system

                // Modes
                mode_normal: AdaptiveColor::from_single(Green),
                mode_insert: AdaptiveColor::from_single(Cyan), // prompt color (insert = typing)
                mode_plan: AdaptiveColor::from_single(Blue),
                mode_auto: AdaptiveColor::from_single(Yellow),

                // Diff
                diff_added: AdaptiveColor::from_single(Rgb(158, 206, 106)), // diff_add
                diff_removed: AdaptiveColor::from_single(Rgb(247, 118, 142)), // diff_del
                diff_added_bg: AdaptiveColor::from_single(Rgb(30, 50, 30)), // subtle green tint
                diff_removed_bg: AdaptiveColor::from_single(Rgb(50, 30, 30)), // subtle red tint
                diff_context: AdaptiveColor::from_single(Rgb(100, 110, 130)), // diff_ctx

                // Overlay
                popup_bg: AdaptiveColor::from_single(Rgb(30, 34, 42)), // popup_bg
                popup_selected_bg: AdaptiveColor::from_single(Rgb(50, 56, 68)), // popup_selected_bg
                toast_bg: AdaptiveColor::from_single(Rgb(45, 40, 55)), // thinking_bg
                overlay_text: AdaptiveColor::from_single(Rgb(192, 202, 245)),
                overlay_bright: AdaptiveColor::from_single(Rgb(255, 255, 255)),

                // Markdown rendering
                code_inline: AdaptiveColor::from_single(Yellow),
                code_fallback: AdaptiveColor::from_single(Green),
                fence_marker: AdaptiveColor::from_single(DarkGray),
                bullet_prefix: AdaptiveColor::from_single(DarkGray),
                blockquote_prefix: AdaptiveColor::from_single(DarkGray),
                blockquote_text: AdaptiveColor::from_single(Gray),
                link: AdaptiveColor::from_single(Blue),
                heading_1: AdaptiveColor::from_single(Cyan),
                heading_2: AdaptiveColor::from_single(Blue),
                heading_3: AdaptiveColor::from_single(Magenta),
            },
            decorations: ThemeDecorations {
                border_style: BorderStyle::Rounded,
                message_user_indicator: "▌".to_string(),
                message_assistant_indicator: " ".to_string(),
                tool_pending_icon: "●".to_string(),
                tool_success_icon: "✓".to_string(),
                tool_error_icon: "✗".to_string(),
                bullet_char: "•".to_string(),
                divider_char: "─".to_string(),
                check_char: "✓".to_string(),
                error_char: "✗".to_string(),
                separator_char: "│".to_string(),
                half_block_top: '▀',
                half_block_bottom: '▄',
            },
            icons: ThemeIcons {
                check: "✓".to_string(),
                error: "✖".to_string(),
                warning: "⚠".to_string(),
                info: "ℹ".to_string(),
                loading: "…".to_string(),
                arrow_right: "→".to_string(),
            },
            spinner: ThemeSpinnerStyle::Braille,
            layout: ThemeLayout::default(),
        }
    }

    /// Resolve an [`AdaptiveColor`] to a concrete [`Color`] using this theme's
    /// `is_dark` setting.
    ///
    /// Delegates to [`AdaptiveColor::resolve()`], which also respects `NO_COLOR`.
    pub fn resolve_color(&self, color: AdaptiveColor) -> Color {
        color.resolve(self.is_dark)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lua theme loading
// ─────────────────────────────────────────────────────────────────────────────

/// Load a ThemeConfig from a Lua string.
///
/// Missing fields use defaults from [`ThemeConfig::default_dark()`].
/// Invalid fields log a warning and use defaults (never panic).
pub fn load_theme_from_lua(lua_string: &str) -> anyhow::Result<ThemeConfig> {
    let lua = Lua::new();
    let value: Value = lua
        .load(lua_string)
        .eval()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    ThemeConfig::from_lua(value, &lua).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Parse a ThemeConfig directly from a Lua table.
///
/// Missing fields use defaults from [`ThemeConfig::default_dark()`].
/// Invalid fields log a warning and use defaults (never panic).
pub fn parse_theme_from_table(lua: &Lua, table: &Table) -> ThemeConfig {
    match ThemeConfig::from_lua(Value::Table(table.clone()), lua) {
        Ok(config) => config,
        Err(e) => {
            warn!("Invalid theme table, using defaults: {e}");
            ThemeConfig::default_dark()
        }
    }
}

impl FromLua for ThemeConfig {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        let table = match value {
            Value::Table(table) => table,
            _ => {
                return Err(mlua::Error::FromLuaConversionError {
                    from: value.type_name(),
                    to: "ThemeConfig".to_string(),
                    message: Some("expected table".to_string()),
                });
            }
        };

        let mut config = ThemeConfig::default_dark();

        if let Ok(value) = table.get::<Value>("name") {
            match String::from_lua(value, lua) {
                Ok(name) => config.name = name,
                Err(e) => warn!("Invalid theme name, using default: {e}"),
            }
        }

        if let Ok(value) = table.get::<Value>("is_dark") {
            match bool::from_lua(value, lua) {
                Ok(is_dark) => config.is_dark = is_dark,
                Err(e) => warn!("Invalid is_dark value, using default: {e}"),
            }
        }

        if let Ok(value) = table.get::<Value>("colors") {
            match ThemeColors::from_lua(value, lua) {
                Ok(colors) => config.colors = colors,
                Err(e) => warn!("Invalid colors table, using default colors: {e}"),
            }
        }

        if let Ok(value) = table.get::<Value>("decorations") {
            match ThemeDecorations::from_lua(value, lua) {
                Ok(decorations) => config.decorations = decorations,
                Err(e) => warn!("Invalid decorations table, using defaults: {e}"),
            }
        }

        if let Ok(value) = table.get::<Value>("icons") {
            match ThemeIcons::from_lua(value, lua) {
                Ok(icons) => config.icons = icons,
                Err(e) => warn!("Invalid icons table, using defaults: {e}"),
            }
        }

        if let Ok(value) = table.get::<Value>("spinner") {
            match ThemeSpinnerStyle::from_lua(value, lua) {
                Ok(spinner) => config.spinner = spinner,
                Err(e) => warn!("Invalid spinner style, using default: {e}"),
            }
        }

        if let Ok(value) = table.get::<Value>("layout") {
            match ThemeLayout::from_lua(value, lua) {
                Ok(layout) => config.layout = layout,
                Err(e) => warn!("Invalid layout table, using defaults: {e}"),
            }
        }

        Ok(config)
    }
}

impl FromLua for ThemeColors {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Table(table) => {
                let mut colors = ThemeConfig::default_dark().colors;
                parse_colors_into(&table, &mut colors);
                Ok(colors)
            }
            Value::Nil => Ok(ThemeConfig::default_dark().colors),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ThemeColors".to_string(),
                message: Some("expected table or nil".to_string()),
            }),
        }
    }
}

impl FromLua for ThemeDecorations {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Table(table) => {
                let mut decorations = ThemeConfig::default_dark().decorations;
                parse_decorations_into(&table, &mut decorations);
                Ok(decorations)
            }
            Value::Nil => Ok(ThemeConfig::default_dark().decorations),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ThemeDecorations".to_string(),
                message: Some("expected table or nil".to_string()),
            }),
        }
    }
}

impl FromLua for ThemeIcons {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Table(table) => {
                let mut icons = ThemeConfig::default_dark().icons;
                parse_icons_into(&table, &mut icons);
                Ok(icons)
            }
            Value::Nil => Ok(ThemeConfig::default_dark().icons),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ThemeIcons".to_string(),
                message: Some("expected table or nil".to_string()),
            }),
        }
    }
}

impl FromLua for ThemeLayout {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Table(table) => {
                let mut layout = ThemeLayout::default();
                parse_layout_into(&table, &mut layout);
                Ok(layout)
            }
            Value::Nil => Ok(ThemeLayout::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ThemeLayout".to_string(),
                message: Some("expected table or nil".to_string()),
            }),
        }
    }
}

impl FromLua for ThemeSpinnerStyle {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::String(_) => {
                let spinner = String::from_lua(value, lua)?;
                parse_spinner_style(&spinner).ok_or_else(|| mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "ThemeSpinnerStyle".to_string(),
                    message: Some(format!("unknown spinner style '{spinner}'")),
                })
            }
            Value::Nil => Ok(ThemeSpinnerStyle::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ThemeSpinnerStyle".to_string(),
                message: Some("expected string or nil".to_string()),
            }),
        }
    }
}

impl FromLua for BorderStyle {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::String(_) => {
                let border_style = String::from_lua(value, lua)?;
                parse_border_style(&border_style).ok_or_else(|| {
                    mlua::Error::FromLuaConversionError {
                        from: "string",
                        to: "BorderStyle".to_string(),
                        message: Some(format!("unknown border style '{border_style}'")),
                    }
                })
            }
            Value::Nil => Ok(BorderStyle::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "BorderStyle".to_string(),
                message: Some("expected string or nil".to_string()),
            }),
        }
    }
}

impl FromLua for StatusBarPosition {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::String(_) => {
                let position = String::from_lua(value, lua)?;
                parse_status_bar_position(&position).ok_or_else(|| {
                    mlua::Error::FromLuaConversionError {
                        from: "string",
                        to: "StatusBarPosition".to_string(),
                        message: Some(format!("unknown status bar position '{position}'")),
                    }
                })
            }
            Value::Nil => Ok(StatusBarPosition::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "StatusBarPosition".to_string(),
                message: Some("expected string or nil".to_string()),
            }),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsing helpers
// ─────────────────────────────────────────────────────────────────────────────

fn parse_adaptive_color(value: &Value) -> Option<AdaptiveColor> {
    match value {
        Value::String(s) => {
            let color = parse_color_string(&s.to_str().ok()?)?;
            Some(AdaptiveColor::from_single(color))
        }
        Value::Table(t) => {
            let dark_str: String = t.get("dark").ok()?;
            let light_str: String = t.get("light").ok()?;
            let dark = parse_color_string(&dark_str)?;
            let light = parse_color_string(&light_str)?;
            Some(AdaptiveColor { dark, light })
        }
        _ => None,
    }
}

fn parse_color_string(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "dark_gray" | "darkgray" | "dark_grey" | "darkgrey" => Some(Color::DarkGray),
        "reset" => Some(Color::Reset),
        _ => None,
    }
}

fn parse_colors_into(table: &Table, colors: &mut ThemeColors) {
    macro_rules! parse_color_field {
        ($field:ident) => {
            if let Ok(val) = table.get::<Value>(stringify!($field)) {
                match parse_adaptive_color(&val) {
                    Some(c) => colors.$field = c,
                    None => {
                        warn!("Invalid color for '{}', using default", stringify!($field));
                    }
                }
            }
        };
    }
    parse_color_field!(primary);
    parse_color_field!(secondary);
    parse_color_field!(background);
    parse_color_field!(background_panel);
    parse_color_field!(command_bg);
    parse_color_field!(shell_bg);
    parse_color_field!(text);
    parse_color_field!(text_muted);
    parse_color_field!(text_dim);
    parse_color_field!(text_emphasized);
    parse_color_field!(error);
    parse_color_field!(warning);
    parse_color_field!(success);
    parse_color_field!(info);
    parse_color_field!(border);
    parse_color_field!(border_focused);
    parse_color_field!(border_dim);
    parse_color_field!(user_message);
    parse_color_field!(assistant_message);
    parse_color_field!(system_message);
    parse_color_field!(mode_normal);
    parse_color_field!(mode_insert);
    parse_color_field!(mode_plan);
    parse_color_field!(mode_auto);
    parse_color_field!(diff_added);
    parse_color_field!(diff_removed);
    parse_color_field!(diff_added_bg);
    parse_color_field!(diff_removed_bg);
    parse_color_field!(diff_context);
    parse_color_field!(popup_bg);
    parse_color_field!(popup_selected_bg);
    parse_color_field!(toast_bg);
    parse_color_field!(overlay_text);
    parse_color_field!(overlay_bright);
    parse_color_field!(code_inline);
    parse_color_field!(code_fallback);
    parse_color_field!(fence_marker);
    parse_color_field!(bullet_prefix);
    parse_color_field!(blockquote_prefix);
    parse_color_field!(blockquote_text);
    parse_color_field!(link);
    parse_color_field!(heading_1);
    parse_color_field!(heading_2);
    parse_color_field!(heading_3);
}

fn parse_decorations_into(table: &Table, dec: &mut ThemeDecorations) {
    if let Ok(s) = table.get::<String>("border_style") {
        match parse_border_style(&s) {
            Some(style) => dec.border_style = style,
            None => warn!("Unknown border_style '{}', using default", s),
        }
    }
    macro_rules! parse_string_field {
        ($field:ident) => {
            if let Ok(s) = table.get::<String>(stringify!($field)) {
                dec.$field = s;
            }
        };
    }
    macro_rules! parse_char_field {
        ($field:ident) => {
            if let Ok(s) = table.get::<String>(stringify!($field)) {
                if let Some(c) = s.chars().next() {
                    dec.$field = c;
                }
            }
        };
    }
    parse_string_field!(message_user_indicator);
    parse_string_field!(message_assistant_indicator);
    parse_string_field!(tool_pending_icon);
    parse_string_field!(tool_success_icon);
    parse_string_field!(tool_error_icon);
    parse_string_field!(bullet_char);
    parse_string_field!(divider_char);
    parse_string_field!(check_char);
    parse_string_field!(error_char);
    parse_string_field!(separator_char);
    parse_char_field!(half_block_top);
    parse_char_field!(half_block_bottom);
}

fn parse_icons_into(table: &Table, icons: &mut ThemeIcons) {
    macro_rules! parse_string_field {
        ($field:ident) => {
            if let Ok(s) = table.get::<String>(stringify!($field)) {
                icons.$field = s;
            }
        };
    }
    parse_string_field!(check);
    parse_string_field!(error);
    parse_string_field!(warning);
    parse_string_field!(info);
    parse_string_field!(loading);
    parse_string_field!(arrow_right);
}

fn parse_layout_into(table: &Table, layout: &mut ThemeLayout) {
    if let Ok(s) = table.get::<String>("status_bar_position") {
        match parse_status_bar_position(&s) {
            Some(position) => layout.status_bar_position = position,
            None => warn!("Unknown status_bar_position '{}', using default", s),
        }
    }
    if let Ok(n) = table.get::<u16>("message_spacing") {
        layout.message_spacing = n;
    }
    if let Ok(n) = table.get::<u16>("code_block_margin") {
        layout.code_block_margin = n;
    }
    if let Ok(n) = table.get::<u16>("input_max_lines") {
        layout.input_max_lines = n;
    }
}

fn parse_spinner_style(s: &str) -> Option<ThemeSpinnerStyle> {
    match s.to_lowercase().as_str() {
        "braille" => Some(ThemeSpinnerStyle::Braille),
        "braille_minidot" => Some(ThemeSpinnerStyle::BrailleMinidot),
        "ascii" => Some(ThemeSpinnerStyle::Ascii),
        "pulse" => Some(ThemeSpinnerStyle::Pulse),
        "none" => Some(ThemeSpinnerStyle::None),
        _ => None,
    }
}

fn parse_border_style(s: &str) -> Option<BorderStyle> {
    match s.to_lowercase().as_str() {
        "rounded" => Some(BorderStyle::Rounded),
        "sharp" => Some(BorderStyle::Sharp),
        "double" => Some(BorderStyle::Double),
        "thick" => Some(BorderStyle::Thick),
        "ascii" => Some(BorderStyle::Ascii),
        "hidden" => Some(BorderStyle::Hidden),
        _ => None,
    }
}

fn parse_status_bar_position(s: &str) -> Option<StatusBarPosition> {
    match s.to_lowercase().as_str() {
        "top" => Some(StatusBarPosition::Top),
        "bottom" => Some(StatusBarPosition::Bottom),
        "hidden" => Some(StatusBarPosition::Hidden),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme_loads() {
        let lua = include_str!("../../../runtime/themes/default.lua");
        let config = load_theme_from_lua(lua).expect("default theme should load");
        assert_eq!(config.name, "default");
        assert!(config.is_dark);
    }

    #[test]
    fn test_theme_partial_override() {
        let lua = r##"return { colors = { error = "#ff0000" } }"##;
        let config = load_theme_from_lua(lua).expect("partial theme should load");
        let default = ThemeConfig::default_dark();
        // Error should be overridden
        assert_ne!(config.colors.error, default.colors.error);
        // Everything else should be default
        assert_eq!(config.colors.success, default.colors.success);
        assert_eq!(config.colors.primary, default.colors.primary);
    }

    #[test]
    fn test_theme_malformed_graceful() {
        let lua = r#"return { colors = { error = "not_a_valid_color_xyz" } }"#;
        // Should not panic, should use default
        let config = load_theme_from_lua(lua).expect("malformed theme should not error");
        let default = ThemeConfig::default_dark();
        assert_eq!(config.colors.error, default.colors.error);
    }

    #[test]
    fn test_theme_named_colors() {
        let cases = [
            ("black", Color::Black),
            ("red", Color::Red),
            ("green", Color::Green),
            ("yellow", Color::Yellow),
            ("blue", Color::Blue),
            ("magenta", Color::Magenta),
            ("cyan", Color::Cyan),
            ("white", Color::White),
            ("gray", Color::Gray),
            ("grey", Color::Gray),
            ("dark_gray", Color::DarkGray),
            ("darkgray", Color::DarkGray),
            ("dark_grey", Color::DarkGray),
            ("darkgrey", Color::DarkGray),
            ("reset", Color::Reset),
        ];

        for (name, expected) in cases {
            let lua = format!(r#"return {{ colors = {{ primary = "{name}" }} }}"#);
            let config = load_theme_from_lua(&lua).expect("named color should load");
            assert_eq!(config.colors.primary, AdaptiveColor::from_single(expected));
        }
    }

    #[test]
    fn test_theme_hex_colors() {
        let lua = r##"return { colors = { error = "#ff0000" } }"##;
        let config = load_theme_from_lua(lua).expect("hex color should load");
        assert_eq!(
            config.colors.error,
            AdaptiveColor::from_single(Color::Rgb(255, 0, 0))
        );
    }

    #[test]
    fn test_theme_adaptive_color() {
        let lua = r##"return { colors = { primary = { dark = "#ffffff", light = "#000000" } } }"##;
        let config = load_theme_from_lua(lua).expect("adaptive color should load");
        assert_eq!(
            config.colors.primary,
            AdaptiveColor {
                dark: Color::Rgb(255, 255, 255),
                light: Color::Rgb(0, 0, 0),
            }
        );
    }

    #[test]
    fn test_theme_from_lua_conversion() {
        let lua = mlua::Lua::new();
        let value: mlua::Value = lua
            .load(r##"return { name = "from-lua", colors = { warning = "#ffaa00" } }"##)
            .eval()
            .expect("valid lua");

        let config = ThemeConfig::from_lua(value, &lua).expect("FromLua conversion should work");
        assert_eq!(config.name, "from-lua");
        assert_eq!(
            config.colors.warning,
            AdaptiveColor::from_single(Color::Rgb(255, 170, 0))
        );
    }

    #[test]
    fn test_default_dark_is_dark() {
        let config = ThemeConfig::default_dark();
        assert!(config.is_dark);
        assert_eq!(config.name, "crucible-dark");
    }

    #[test]
    fn test_resolve_color_respects_is_dark() {
        let mut config = ThemeConfig::default_dark();
        let test_color = AdaptiveColor {
            dark: Color::Red,
            light: Color::Blue,
        };

        // Dark mode → dark variant
        assert_eq!(config.resolve_color(test_color), Color::Red);

        // Light mode → light variant
        config.is_dark = false;
        assert_eq!(config.resolve_color(test_color), Color::Blue);
    }

    #[test]
    fn test_spinner_frames() {
        assert_eq!(ThemeSpinnerStyle::Braille.frames().len(), 10);
        assert_eq!(ThemeSpinnerStyle::Ascii.frames().len(), 4);
        assert_eq!(ThemeSpinnerStyle::Ascii.frames(), &['-', '\\', '|', '/']);
        assert!(ThemeSpinnerStyle::None.frames().is_empty());
    }

    #[test]
    fn test_border_style_default_is_rounded() {
        assert_eq!(BorderStyle::default(), BorderStyle::Rounded);
    }

    #[test]
    fn test_spinner_style_default_is_braille() {
        assert_eq!(ThemeSpinnerStyle::default(), ThemeSpinnerStyle::Braille);
    }

    #[test]
    fn test_default_layout_values() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.layout.status_bar_position, StatusBarPosition::Bottom);
        assert_eq!(config.layout.message_spacing, 1);
        assert_eq!(config.layout.input_max_lines, 6);
    }

    #[test]
    fn test_decorations_parsing() {
        let lua = r#"return { decorations = { border_style = "sharp", bullet_char = "*" } }"#;
        let config = load_theme_from_lua(lua).expect("decorations should parse");
        assert_eq!(config.decorations.border_style, BorderStyle::Sharp);
        assert_eq!(config.decorations.bullet_char, "*");
        // Default preserved
        assert_eq!(config.decorations.check_char, "✓");
    }

    #[test]
    fn test_layout_parsing() {
        let lua = r#"return { layout = { status_bar_position = "top", message_spacing = 2 } }"#;
        let config = load_theme_from_lua(lua).expect("layout should parse");
        assert_eq!(config.layout.status_bar_position, StatusBarPosition::Top);
        assert_eq!(config.layout.message_spacing, 2);
    }

    #[test]
    fn test_icons_parsing() {
        let lua = r#"return { icons = { check = "OK", error = "FAIL" } }"#;
        let config = load_theme_from_lua(lua).expect("icons should parse");
        assert_eq!(config.icons.check, "OK");
        assert_eq!(config.icons.error, "FAIL");
        // Default preserved
        assert_eq!(config.icons.warning, "⚠");
    }

    #[test]
    fn test_spinner_parsing() {
        let lua = r#"return { spinner = "pulse" }"#;
        let config = load_theme_from_lua(lua).expect("spinner should parse");
        assert_eq!(config.spinner, ThemeSpinnerStyle::Pulse);
    }

    #[test]
    fn test_invalid_lua_returns_error() {
        let lua = "this is not valid lua {{{";
        assert!(load_theme_from_lua(lua).is_err());
    }

    #[test]
    fn test_non_table_returns_error() {
        let lua = r#"return "not a table""#;
        assert!(load_theme_from_lua(lua).is_err());
    }

    #[test]
    fn test_default_theme_loads_all_tokens() {
        let lua_src = include_str!("../../../runtime/themes/default.lua");
        let config = load_theme_from_lua(lua_src).expect("default theme should load");
        let dark = ThemeConfig::default_dark();

        // Metadata
        assert_eq!(config.name, "default");
        assert!(config.is_dark);

        // Verify all color fields are populated by comparing resolved colors
        // against a non-default value (Color::Reset means unset/fallback).
        // We spot-check fields from each section to ensure the Lua file covers them.

        // Core
        assert_ne!(config.resolve_color(config.colors.primary), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.background), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.command_bg), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.shell_bg), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.text_dim), Color::Reset);

        // Semantic
        assert_ne!(config.resolve_color(config.colors.error), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.success), Color::Reset);

        // Overlay (including overlay_bright)
        assert_ne!(config.resolve_color(config.colors.overlay_text), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.overlay_bright), Color::Reset);

        // Markdown
        assert_ne!(config.resolve_color(config.colors.code_inline), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.heading_1), Color::Reset);
        assert_ne!(config.resolve_color(config.colors.link), Color::Reset);

        // Decorations
        assert_eq!(config.decorations.border_style, dark.decorations.border_style);
        assert_eq!(config.decorations.message_user_indicator, dark.decorations.message_user_indicator);

        // Layout
        assert_eq!(config.layout.status_bar_position, dark.layout.status_bar_position);
        assert_eq!(config.layout.message_spacing, dark.layout.message_spacing);
        assert_eq!(config.layout.input_max_lines, dark.layout.input_max_lines);

        // Spinner
        assert_eq!(config.spinner, ThemeSpinnerStyle::Braille);
    }

    #[test]
    fn test_opencode_theme_loads() {
        let lua_src = include_str!("../../../runtime/themes/opencode.lua");
        let theme = load_theme_from_lua(lua_src).expect("opencode theme should load");
        assert_eq!(theme.name, "opencode");
        assert!(theme.is_dark);
        // Verify it differs from default
        let default = ThemeConfig::default_dark();
        // primary should be different (fab283 vs cyan)
        assert_ne!(
            theme.resolve_color(theme.colors.primary),
            default.resolve_color(default.colors.primary),
            "opencode primary should differ from default"
        );
        // border_style should be sharp
        assert_eq!(theme.decorations.border_style, BorderStyle::Sharp);
    }
}
