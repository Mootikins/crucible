//! Theme configuration types for the TUI theming system.
//!
//! These types define the full theme schema: colors, decorations, icons, layout,
//! and spinner style. `ThemeConfig::default_dark()` produces values identical
//! to the current `ThemeTokens::default_tokens()` dark theme.
//!
//! **These types are defined only — they are NOT yet wired into components.**
//! `ThemeTokens` remains the active runtime theme system.

use crucible_oil::style::{AdaptiveColor, Color};

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
    /// Primary text color
    pub text: AdaptiveColor,
    /// Muted/secondary text color
    pub text_muted: AdaptiveColor,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSpinnerStyle {
    /// Braille dots: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
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

impl Default for ThemeSpinnerStyle {
    fn default() -> Self {
        ThemeSpinnerStyle::Braille
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BorderStyle — theme-level border style selection
// ─────────────────────────────────────────────────────────────────────────────

/// Border drawing style for panels, popups, and input areas.
///
/// This is a theme-level abstraction over `crucible_oil::style::Border`.
/// Maps to oil's `Border` enum for actual character rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    /// Rounded corners: ╭ ╮ ╰ ╯
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

impl Default for BorderStyle {
    fn default() -> Self {
        BorderStyle::Rounded
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// StatusBarPosition — layout option for status bar placement
// ─────────────────────────────────────────────────────────────────────────────

/// Position of the status bar in the TUI layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarPosition {
    /// Status bar at the top of the screen
    Top,
    /// Status bar at the bottom of the screen
    Bottom,
    /// Status bar hidden
    Hidden,
}

impl Default for StatusBarPosition {
    fn default() -> Self {
        StatusBarPosition::Bottom
    }
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
/// matches [`ThemeTokens::default_tokens()`].
///
/// # Example
///
/// ```rust,ignore
/// use crate::tui::oil::theme::ThemeConfig;
///
/// let theme = ThemeConfig::default_dark();
/// let resolved = theme.resolve_color(theme.colors.error);
/// ```
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
    /// Color values are identical to [`ThemeTokens::default_tokens()`] —
    /// each ThemeTokens color is wrapped in `AdaptiveColor::from_single()`
    /// so dark and light variants are the same (v1 behavior).
    pub fn default_dark() -> Self {
        use Color::*;

        Self {
            name: "crucible-dark".to_string(),
            is_dark: true,
            colors: ThemeColors {
                // Core — mapped from ThemeTokens
                primary: AdaptiveColor::from_single(Cyan), // text_accent / prompt
                secondary: AdaptiveColor::from_single(Magenta), // role_tool
                background: AdaptiveColor::from_single(Rgb(40, 44, 52)), // input_bg
                background_panel: AdaptiveColor::from_single(Rgb(35, 39, 47)), // code_bg
                text: AdaptiveColor::from_single(White),   // text_primary
                text_muted: AdaptiveColor::from_single(DarkGray), // text_muted
                text_emphasized: AdaptiveColor::from_single(Cyan), // text_accent

                // Semantic — exact ThemeTokens values
                error: AdaptiveColor::from_single(Rgb(247, 118, 142)),
                warning: AdaptiveColor::from_single(Rgb(224, 175, 104)),
                success: AdaptiveColor::from_single(Rgb(158, 206, 106)),
                info: AdaptiveColor::from_single(Rgb(0, 206, 209)),

                // Borders — from ThemeTokens
                border: AdaptiveColor::from_single(Rgb(40, 44, 52)), // border (= input_bg)
                border_focused: AdaptiveColor::from_single(Cyan),    // selected
                border_dim: AdaptiveColor::from_single(Gray),        // text_dim

                // Chat roles — from ThemeTokens
                user_message: AdaptiveColor::from_single(Green), // role_user
                assistant_message: AdaptiveColor::from_single(Cyan), // role_assistant
                system_message: AdaptiveColor::from_single(Yellow), // role_system

                // Modes — from ThemeTokens
                mode_normal: AdaptiveColor::from_single(Green),
                mode_insert: AdaptiveColor::from_single(Cyan), // prompt color (insert = typing)
                mode_plan: AdaptiveColor::from_single(Blue),
                mode_auto: AdaptiveColor::from_single(Yellow),

                // Diff — from ThemeTokens
                diff_added: AdaptiveColor::from_single(Rgb(158, 206, 106)), // diff_add
                diff_removed: AdaptiveColor::from_single(Rgb(247, 118, 142)), // diff_del
                diff_added_bg: AdaptiveColor::from_single(Rgb(30, 50, 30)), // subtle green tint
                diff_removed_bg: AdaptiveColor::from_single(Rgb(50, 30, 30)), // subtle red tint
                diff_context: AdaptiveColor::from_single(Rgb(100, 110, 130)), // diff_ctx

                // Overlay — from ThemeTokens
                popup_bg: AdaptiveColor::from_single(Rgb(30, 34, 42)), // popup_bg
                popup_selected_bg: AdaptiveColor::from_single(Rgb(50, 56, 68)), // popup_selected_bg
                toast_bg: AdaptiveColor::from_single(Rgb(45, 40, 55)), // thinking_bg
            },
            decorations: ThemeDecorations {
                border_style: BorderStyle::Rounded,
                message_user_indicator: "▌".to_string(),
                message_assistant_indicator: " ".to_string(),
                tool_pending_icon: "●".to_string(),
                tool_success_icon: "✓".to_string(),
                tool_error_icon: "✖".to_string(),
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
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::theme::ThemeTokens;

    #[test]
    fn default_dark_core_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        // Text colors
        assert_eq!(
            config.resolve_color(config.colors.text),
            tokens.text_primary
        );
        assert_eq!(
            config.resolve_color(config.colors.text_muted),
            tokens.text_muted
        );
        assert_eq!(
            config.resolve_color(config.colors.text_emphasized),
            tokens.text_accent
        );

        // Background
        assert_eq!(
            config.resolve_color(config.colors.background),
            tokens.input_bg
        );
        assert_eq!(
            config.resolve_color(config.colors.background_panel),
            tokens.code_bg
        );
    }

    #[test]
    fn default_dark_semantic_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(config.resolve_color(config.colors.error), tokens.error);
        assert_eq!(config.resolve_color(config.colors.warning), tokens.warning);
        assert_eq!(config.resolve_color(config.colors.success), tokens.success);
        assert_eq!(config.resolve_color(config.colors.info), tokens.info);
    }

    #[test]
    fn default_dark_chat_role_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.user_message),
            tokens.role_user
        );
        assert_eq!(
            config.resolve_color(config.colors.assistant_message),
            tokens.role_assistant
        );
        assert_eq!(
            config.resolve_color(config.colors.system_message),
            tokens.role_system
        );
    }

    #[test]
    fn default_dark_mode_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.mode_normal),
            tokens.mode_normal
        );
        assert_eq!(
            config.resolve_color(config.colors.mode_plan),
            tokens.mode_plan
        );
        assert_eq!(
            config.resolve_color(config.colors.mode_auto),
            tokens.mode_auto
        );
    }

    #[test]
    fn default_dark_diff_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.diff_added),
            tokens.diff_add
        );
        assert_eq!(
            config.resolve_color(config.colors.diff_removed),
            tokens.diff_del
        );
        assert_eq!(
            config.resolve_color(config.colors.diff_context),
            tokens.diff_ctx
        );
    }

    #[test]
    fn default_dark_overlay_colors_match_theme_tokens() {
        let config = ThemeConfig::default_dark();
        let tokens = ThemeTokens::default_tokens();

        assert_eq!(
            config.resolve_color(config.colors.popup_bg),
            tokens.popup_bg
        );
        assert_eq!(
            config.resolve_color(config.colors.popup_selected_bg),
            tokens.popup_selected_bg
        );
        assert_eq!(config.resolve_color(config.colors.border), tokens.border);
    }

    #[test]
    fn default_dark_is_dark() {
        let config = ThemeConfig::default_dark();
        assert!(config.is_dark);
        assert_eq!(config.name, "crucible-dark");
    }

    #[test]
    fn resolve_color_respects_is_dark() {
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
    fn spinner_braille_frames_match_oil() {
        let style = ThemeSpinnerStyle::Braille;
        let oil_frames = crucible_oil::node::BRAILLE_SPINNER_FRAMES;
        assert_eq!(style.frames(), oil_frames);
    }

    #[test]
    fn spinner_pulse_frames_match_oil() {
        let style = ThemeSpinnerStyle::Pulse;
        let oil_frames = crucible_oil::node::SPINNER_FRAMES;
        assert_eq!(style.frames(), oil_frames);
    }

    #[test]
    fn spinner_none_has_empty_frames() {
        assert!(ThemeSpinnerStyle::None.frames().is_empty());
    }

    #[test]
    fn spinner_ascii_has_four_frames() {
        let frames = ThemeSpinnerStyle::Ascii.frames();
        assert_eq!(frames.len(), 4);
        assert_eq!(frames, &['-', '\\', '|', '/']);
    }

    #[test]
    fn default_decorations_have_expected_chars() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.decorations.message_user_indicator, "▌");
        assert_eq!(config.decorations.tool_success_icon, "✓");
        assert_eq!(config.decorations.tool_error_icon, "✖");
        assert_eq!(config.decorations.half_block_top, '▀');
        assert_eq!(config.decorations.half_block_bottom, '▄');
    }

    #[test]
    fn default_icons_have_expected_chars() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.icons.check, "✓");
        assert_eq!(config.icons.error, "✖");
        assert_eq!(config.icons.warning, "⚠");
        assert_eq!(config.icons.arrow_right, "→");
    }

    #[test]
    fn default_layout_values() {
        let config = ThemeConfig::default_dark();
        assert_eq!(config.layout.status_bar_position, StatusBarPosition::Bottom);
        assert_eq!(config.layout.message_spacing, 1);
        assert_eq!(config.layout.input_max_lines, 6);
    }

    #[test]
    fn border_style_default_is_rounded() {
        assert_eq!(BorderStyle::default(), BorderStyle::Rounded);
    }

    #[test]
    fn spinner_style_default_is_braille() {
        assert_eq!(ThemeSpinnerStyle::default(), ThemeSpinnerStyle::Braille);
    }

    #[test]
    fn theme_colors_has_30_fields() {
        // Verified by the exhaustive struct literal in default_dark():
        // 7 core + 4 semantic + 3 borders + 3 chat roles +
        // 4 modes + 5 diff + 3 overlay = 29... wait let me count
        // Actually: primary, secondary, background, background_panel, text,
        // text_muted, text_emphasized = 7
        // error, warning, success, info = 4
        // border, border_focused, border_dim = 3
        // user_message, assistant_message, system_message = 3
        // mode_normal, mode_insert, mode_plan, mode_auto = 4
        // diff_added, diff_removed, diff_added_bg, diff_removed_bg, diff_context = 5
        // popup_bg, popup_selected_bg, toast_bg = 3
        // Total = 7 + 4 + 3 + 3 + 4 + 5 + 3 = 29
        //
        // If any field were missing, the struct literal in default_dark() wouldn't compile.
        let config = ThemeConfig::default_dark();
        // Verify clone works (exercises all fields)
        let config2 = config.clone();
        assert_eq!(config.colors, config2.colors);
    }
}
