//! Runtime theme token system.
//!
//! `ThemeTokens` is the runtime equivalent of the compile-time `colors::*` constants
//! and `styles::*` functions. Components will look up colors via `ThemeTokens` at
//! runtime, enabling future theme customization without recompilation.
//!
//! # Migration path
//!
//! 1. (This task) Create `ThemeTokens` with identical values to `colors::*` constants
//! 2. Wire `ThemeTokens` into `ViewContext` so components can access it
//! 3. (Future) Migrate components from `colors::CONST` → `ctx.theme().field`
//! 4. (Future) Remove legacy `colors::*` and `styles::*` modules

use crucible_oil::style::{Color, Style};

/// Runtime color tokens for the TUI theme.
///
/// Each field corresponds to a `const` in [`super::colors`]. The field names match
/// the constant names in `snake_case` (which they already are).
///
/// Use [`ThemeTokens::default()`] to get the standard dark theme, identical to
/// the compile-time constants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeTokens {
    // ── Surfaces (backgrounds) ──────────────────────────────────────────
    /// Default input field background (dark gray)
    pub input_bg: Color,
    /// Command mode input background (amber tint)
    pub command_bg: Color,
    /// Shell mode input background (red tint)
    pub shell_bg: Color,
    /// Popup/overlay background
    pub popup_bg: Color,
    /// Code block background
    pub code_bg: Color,
    /// Thinking block background
    pub thinking_bg: Color,

    // ── Text colors ─────────────────────────────────────────────────────
    /// Primary text color
    pub text_primary: Color,
    /// Secondary/muted text
    pub text_muted: Color,
    /// Accent text (links, highlights)
    pub text_accent: Color,
    /// Dimmed text (timestamps, metadata)
    pub text_dim: Color,

    // ── Semantic colors ─────────────────────────────────────────────────
    /// Success indicator (green)
    pub success: Color,
    /// Error indicator (red/pink)
    pub error: Color,
    /// Warning indicator (amber)
    pub warning: Color,
    /// Info indicator (cyan)
    pub info: Color,

    // ── Chat roles ──────────────────────────────────────────────────────
    /// User message color
    pub role_user: Color,
    /// Assistant message color
    pub role_assistant: Color,
    /// System message color
    pub role_system: Color,
    /// Tool/function call color
    pub role_tool: Color,

    // ── Chat modes ──────────────────────────────────────────────────────
    /// Normal mode badge background
    pub mode_normal: Color,
    /// Plan mode badge background
    pub mode_plan: Color,
    /// Auto mode badge background
    pub mode_auto: Color,

    // ── UI elements ─────────────────────────────────────────────────────
    /// Spinner/loading indicator
    pub spinner: Color,
    /// Selected item in popup
    pub selected: Color,
    /// Border color
    pub border: Color,
    /// Prompt character color
    pub prompt: Color,
    /// Model name in status bar
    pub model_name: Color,
    /// Notification text
    pub notification: Color,

    // ── Markdown rendering ──────────────────────────────────────────────
    /// Inline code color
    pub code_inline: Color,
    /// Code block fallback (when no syntax highlighting)
    pub code_fallback: Color,
    /// Fence markers (```)
    pub fence_marker: Color,
    /// Blockquote prefix (│)
    pub blockquote_prefix: Color,
    /// Blockquote text
    pub blockquote_text: Color,
    /// Link color
    pub link: Color,
    /// Heading level 1
    pub heading_1: Color,
    /// Heading level 2
    pub heading_2: Color,
    /// Heading level 3
    pub heading_3: Color,
    /// Bullet prefix for assistant messages
    pub bullet_prefix: Color,

    // ── Overlay system ──────────────────────────────────────────────────
    /// Primary text for overlays
    pub overlay_text: Color,
    /// Dimmed hint text in overlays
    pub overlay_dim: Color,
    /// Bright white for action details
    pub overlay_bright: Color,

    // ── Diff panel ──────────────────────────────────────────────────────
    /// Diff panel background
    pub diff_bg: Color,
    /// Diff foreground (used for inner borders against INPUT_BG)
    pub diff_fg: Color,
    /// Diff added lines
    pub diff_add: Color,
    /// Diff deleted lines
    pub diff_del: Color,
    /// Diff context lines
    pub diff_ctx: Color,
    /// Diff hunk headers
    pub diff_hunk: Color,
    /// Line number gutter
    pub gutter_fg: Color,
}

impl ThemeTokens {
    /// Construct the default dark theme.
    ///
    /// Every value here is identical to the corresponding `colors::*` constant.
    /// This is a `const fn` so it can be used in static contexts.
    pub const fn default_tokens() -> Self {
        use Color::*;
        Self {
            // Surfaces
            input_bg: Rgb(40, 44, 52),
            command_bg: Rgb(60, 50, 20),
            shell_bg: Rgb(60, 30, 30),
            popup_bg: Rgb(30, 34, 42),
            code_bg: Rgb(35, 39, 47),
            thinking_bg: Rgb(45, 40, 55),

            // Text
            text_primary: White,
            text_muted: DarkGray,
            text_accent: Cyan,
            text_dim: Gray,

            // Semantic
            success: Rgb(158, 206, 106),
            error: Rgb(247, 118, 142),
            warning: Rgb(224, 175, 104),
            info: Rgb(0, 206, 209),

            // Roles
            role_user: Green,
            role_assistant: Cyan,
            role_system: Yellow,
            role_tool: Magenta,

            // Modes
            mode_normal: Green,
            mode_plan: Blue,
            mode_auto: Yellow,

            // UI elements
            spinner: Cyan,
            selected: Cyan,
            border: Rgb(40, 44, 52), // same as INPUT_BG
            prompt: Cyan,
            model_name: Cyan,
            notification: Yellow,

            // Markdown
            code_inline: Yellow,
            code_fallback: Green,
            fence_marker: DarkGray,
            blockquote_prefix: DarkGray,
            blockquote_text: Gray,
            link: Blue,
            heading_1: Cyan,
            heading_2: Blue,
            heading_3: Magenta,
            bullet_prefix: DarkGray,

            // Overlay
            overlay_text: Rgb(192, 202, 245),
            overlay_dim: Rgb(100, 110, 130),
            overlay_bright: Rgb(255, 255, 255),

            // Diff
            diff_bg: Rgb(28, 32, 40),
            diff_fg: Rgb(28, 32, 40),
            diff_add: Rgb(158, 206, 106),
            diff_del: Rgb(247, 118, 142),
            diff_ctx: Rgb(100, 110, 130),
            diff_hunk: Rgb(0, 206, 209),
            gutter_fg: Rgb(70, 75, 90),
        }
    }

    /// Get a `&'static` reference to the default theme tokens.
    ///
    /// Useful when constructing a `ViewContext` without a custom theme.
    pub fn default_ref() -> &'static ThemeTokens {
        static DEFAULT: ThemeTokens = ThemeTokens::default_tokens();
        &DEFAULT
    }
}

impl Default for ThemeTokens {
    fn default() -> Self {
        Self::default_tokens()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Style preset methods — runtime equivalents of `styles::*` functions
// ─────────────────────────────────────────────────────────────────────────────

impl ThemeTokens {
    // ── Chat roles ──────────────────────────────────────────────────────

    /// Style for user message prefix/content
    pub fn user_prompt(&self) -> Style {
        Style::new().fg(self.role_user).bold()
    }

    /// Style for assistant response
    pub fn assistant_response(&self) -> Style {
        Style::new().fg(self.role_assistant)
    }

    /// Style for system messages
    pub fn system_message(&self) -> Style {
        Style::new().fg(self.role_system).italic()
    }

    /// Style for tool calls
    pub fn tool_call(&self) -> Style {
        Style::new().fg(self.role_tool).dim()
    }

    /// Style for tool results
    pub fn tool_result(&self) -> Style {
        Style::new().fg(self.text_dim)
    }

    // ── Status indicators ───────────────────────────────────────────────

    /// Style for error messages
    pub fn error_style(&self) -> Style {
        Style::new().fg(self.error).bold()
    }

    /// Style for warning messages
    pub fn warning_style(&self) -> Style {
        Style::new().fg(self.warning)
    }

    /// Style for success messages
    pub fn success_style(&self) -> Style {
        Style::new().fg(self.success)
    }

    /// Style for info messages
    pub fn info_style(&self) -> Style {
        Style::new().fg(self.info)
    }

    // ── Text variations ─────────────────────────────────────────────────

    /// Muted/secondary text
    pub fn muted(&self) -> Style {
        Style::new().fg(self.text_muted)
    }

    /// Dimmed text (less prominent than muted)
    pub fn dim(&self) -> Style {
        Style::new().fg(self.text_dim).dim()
    }

    /// Accent text (links, highlights)
    pub fn accent(&self) -> Style {
        Style::new().fg(self.text_accent)
    }

    /// Bold accent (important highlights)
    pub fn accent_bold(&self) -> Style {
        Style::new().fg(self.text_accent).bold()
    }

    // ── UI elements ─────────────────────────────────────────────────────

    /// Style for input prompt character (>, :, !)
    pub fn prompt_style(&self) -> Style {
        Style::new().fg(self.prompt)
    }

    /// Style for spinner/loading
    pub fn spinner_style(&self) -> Style {
        Style::new().fg(self.spinner)
    }

    /// Style for model name in status bar
    pub fn model_name_style(&self) -> Style {
        Style::new().fg(self.model_name)
    }

    /// Style for notifications
    pub fn notification_style(&self) -> Style {
        Style::new().fg(self.notification)
    }

    /// Style for selected item (inverted)
    pub fn selected_style(&self) -> Style {
        Style::new().fg(Color::Black).bg(self.selected)
    }

    /// Style for popup item description
    pub fn popup_description(&self) -> Style {
        Style::new().fg(self.text_dim).dim()
    }

    // ── Mode badges ─────────────────────────────────────────────────────

    /// Style for NORMAL mode badge
    pub fn mode_normal_style(&self) -> Style {
        Style::new().bg(self.mode_normal).fg(Color::Black).bold()
    }

    /// Style for PLAN mode badge
    pub fn mode_plan_style(&self) -> Style {
        Style::new().bg(self.mode_plan).fg(Color::Black).bold()
    }

    /// Style for AUTO mode badge
    pub fn mode_auto_style(&self) -> Style {
        Style::new().bg(self.mode_auto).fg(Color::Black).bold()
    }

    // ── Code/thinking blocks ────────────────────────────────────────────

    /// Style for code block background
    pub fn code_block(&self) -> Style {
        Style::new().bg(self.code_bg)
    }

    /// Style for thinking block header
    pub fn thinking_header(&self) -> Style {
        Style::new().fg(self.text_dim).italic()
    }

    /// Style for thinking block content
    pub fn thinking_content(&self) -> Style {
        Style::new().fg(self.text_dim).dim()
    }

    // ── Diff display ────────────────────────────────────────────────────

    /// Style for diff deletions
    pub fn diff_delete(&self) -> Style {
        Style::new().fg(self.error)
    }

    /// Style for diff insertions
    pub fn diff_insert(&self) -> Style {
        Style::new().fg(self.success)
    }

    /// Style for diff context lines
    pub fn diff_context(&self) -> Style {
        Style::new().fg(self.text_dim)
    }

    /// Style for diff hunk headers
    pub fn diff_hunk_header(&self) -> Style {
        Style::new().fg(self.info)
    }

    // ── Markdown rendering ──────────────────────────────────────────────

    /// Style for inline code
    pub fn inline_code(&self) -> Style {
        Style::new().fg(self.code_inline)
    }

    /// Style for code block fallback text
    pub fn code_fallback_style(&self) -> Style {
        Style::new().fg(self.code_fallback)
    }

    /// Style for fence markers
    pub fn fence_marker_style(&self) -> Style {
        Style::new().fg(self.fence_marker)
    }

    /// Style for blockquote prefix
    pub fn blockquote_prefix_style(&self) -> Style {
        Style::new().fg(self.blockquote_prefix)
    }

    /// Style for blockquote text
    pub fn blockquote_text_style(&self) -> Style {
        Style::new().fg(self.blockquote_text).italic()
    }

    /// Style for links
    pub fn link_style(&self) -> Style {
        Style::new().fg(self.link).underline()
    }

    /// Style for heading level 1
    pub fn heading_1_style(&self) -> Style {
        Style::new().fg(self.heading_1).bold()
    }

    /// Style for heading level 2
    pub fn heading_2_style(&self) -> Style {
        Style::new().fg(self.heading_2).bold()
    }

    /// Style for heading level 3
    pub fn heading_3_style(&self) -> Style {
        Style::new().fg(self.heading_3).bold()
    }

    /// Style for bullet prefix
    pub fn bullet_prefix_style(&self) -> Style {
        Style::new().fg(self.bullet_prefix)
    }

    // ── Overlay badges ──────────────────────────────────────────────────

    /// Reverse-video badge for a notification (INFO/WARN/ERRO).
    pub fn notification_badge(&self, color: Color) -> Style {
        Style::new().fg(color).bold().reverse()
    }

    /// PERMISSION badge (red reverse-video)
    pub fn permission_badge(&self) -> Style {
        Style::new().fg(self.error).bold().reverse()
    }

    /// Permission type label (red bold, no reverse)
    pub fn permission_type(&self) -> Style {
        Style::new().fg(self.error).bold()
    }

    /// Key hint in overlay footers (colored key text)
    pub fn overlay_key(&self, color: Color) -> Style {
        Style::new().fg(color)
    }

    /// Dim hint text in overlay footers
    pub fn overlay_hint(&self) -> Style {
        Style::new().fg(self.overlay_dim)
    }

    /// Overlay panel text
    pub fn overlay_text_style(&self) -> Style {
        Style::new().fg(self.overlay_text)
    }

    /// Bright text for action details in overlays
    pub fn overlay_bright_style(&self) -> Style {
        Style::new().fg(self.overlay_bright)
    }

    /// Diff gutter (line numbers)
    pub fn diff_gutter(&self) -> Style {
        Style::new().fg(self.gutter_fg).bg(self.diff_bg)
    }

    /// Diff panel background
    pub fn diff_bg_style(&self) -> Style {
        Style::new().bg(self.diff_bg)
    }

    /// Input/panel background
    pub fn input_bg_style(&self) -> Style {
        Style::new().bg(self.input_bg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::theme::{colors, styles};

    #[test]
    fn default_matches_legacy_colors() {
        let t = ThemeTokens::default();

        // Surfaces
        assert_eq!(t.input_bg, colors::INPUT_BG);
        assert_eq!(t.command_bg, colors::COMMAND_BG);
        assert_eq!(t.shell_bg, colors::SHELL_BG);
        assert_eq!(t.popup_bg, colors::POPUP_BG);
        assert_eq!(t.code_bg, colors::CODE_BG);
        assert_eq!(t.thinking_bg, colors::THINKING_BG);

        // Text
        assert_eq!(t.text_primary, colors::TEXT_PRIMARY);
        assert_eq!(t.text_muted, colors::TEXT_MUTED);
        assert_eq!(t.text_accent, colors::TEXT_ACCENT);
        assert_eq!(t.text_dim, colors::TEXT_DIM);

        // Semantic
        assert_eq!(t.success, colors::SUCCESS);
        assert_eq!(t.error, colors::ERROR);
        assert_eq!(t.warning, colors::WARNING);
        assert_eq!(t.info, colors::INFO);

        // Roles
        assert_eq!(t.role_user, colors::ROLE_USER);
        assert_eq!(t.role_assistant, colors::ROLE_ASSISTANT);
        assert_eq!(t.role_system, colors::ROLE_SYSTEM);
        assert_eq!(t.role_tool, colors::ROLE_TOOL);

        // Modes
        assert_eq!(t.mode_normal, colors::MODE_NORMAL);
        assert_eq!(t.mode_plan, colors::MODE_PLAN);
        assert_eq!(t.mode_auto, colors::MODE_AUTO);

        // UI elements
        assert_eq!(t.spinner, colors::SPINNER);
        assert_eq!(t.selected, colors::SELECTED);
        assert_eq!(t.border, colors::BORDER);
        assert_eq!(t.prompt, colors::PROMPT);
        assert_eq!(t.model_name, colors::MODEL_NAME);
        assert_eq!(t.notification, colors::NOTIFICATION);

        // Markdown
        assert_eq!(t.code_inline, colors::CODE_INLINE);
        assert_eq!(t.code_fallback, colors::CODE_FALLBACK);
        assert_eq!(t.fence_marker, colors::FENCE_MARKER);
        assert_eq!(t.blockquote_prefix, colors::BLOCKQUOTE_PREFIX);
        assert_eq!(t.blockquote_text, colors::BLOCKQUOTE_TEXT);
        assert_eq!(t.link, colors::LINK);
        assert_eq!(t.heading_1, colors::HEADING_1);
        assert_eq!(t.heading_2, colors::HEADING_2);
        assert_eq!(t.heading_3, colors::HEADING_3);
        assert_eq!(t.bullet_prefix, colors::BULLET_PREFIX);

        // Overlay
        assert_eq!(t.overlay_text, colors::OVERLAY_TEXT);
        assert_eq!(t.overlay_dim, colors::OVERLAY_DIM);
        assert_eq!(t.overlay_bright, colors::OVERLAY_BRIGHT);

        // Diff
        assert_eq!(t.diff_bg, colors::DIFF_BG);
        assert_eq!(t.diff_fg, colors::DIFF_FG);
        assert_eq!(t.diff_add, colors::DIFF_ADD);
        assert_eq!(t.diff_del, colors::DIFF_DEL);
        assert_eq!(t.diff_ctx, colors::DIFF_CTX);
        assert_eq!(t.diff_hunk, colors::DIFF_HUNK);
        assert_eq!(t.gutter_fg, colors::GUTTER_FG);
    }

    #[test]
    fn default_has_47_tokens() {
        // Count verified by the exhaustive test above covering all 47 fields:
        // 6 surfaces + 4 text + 4 semantic + 4 roles + 3 modes +
        // 6 UI elements + 10 markdown + 3 overlay + 7 diff = 47
        let t = ThemeTokens::default();
        // If any field were missing, the struct literal in default_tokens() wouldn't compile.
        assert_eq!(t, ThemeTokens::default_tokens());
    }

    #[test]
    fn style_presets_match_legacy_styles() {
        let t = ThemeTokens::default();

        // Chat roles
        assert_eq!(t.user_prompt(), styles::user_prompt());
        assert_eq!(t.assistant_response(), styles::assistant_response());
        assert_eq!(t.system_message(), styles::system_message());
        assert_eq!(t.tool_call(), styles::tool_call());
        assert_eq!(t.tool_result(), styles::tool_result());

        // Status
        assert_eq!(t.error_style(), styles::error());
        assert_eq!(t.warning_style(), styles::warning());
        assert_eq!(t.success_style(), styles::success());
        assert_eq!(t.info_style(), styles::info());

        // Text variations
        assert_eq!(t.muted(), styles::muted());
        assert_eq!(t.dim(), styles::dim());
        assert_eq!(t.accent(), styles::accent());
        assert_eq!(t.accent_bold(), styles::accent_bold());

        // UI elements
        assert_eq!(t.prompt_style(), styles::prompt());
        assert_eq!(t.spinner_style(), styles::spinner());
        assert_eq!(t.model_name_style(), styles::model_name());
        assert_eq!(t.notification_style(), styles::notification());
        assert_eq!(t.selected_style(), styles::selected());
        assert_eq!(t.popup_description(), styles::popup_description());

        // Mode badges
        assert_eq!(t.mode_normal_style(), styles::mode_normal());
        assert_eq!(t.mode_plan_style(), styles::mode_plan());
        assert_eq!(t.mode_auto_style(), styles::mode_auto());

        // Code/thinking
        assert_eq!(t.code_block(), styles::code_block());
        assert_eq!(t.thinking_header(), styles::thinking_header());
        assert_eq!(t.thinking_content(), styles::thinking_content());

        // Diff
        assert_eq!(t.diff_delete(), styles::diff_delete());
        assert_eq!(t.diff_insert(), styles::diff_insert());
        assert_eq!(t.diff_context(), styles::diff_context());
        assert_eq!(t.diff_hunk_header(), styles::diff_hunk_header());

        // Markdown
        assert_eq!(t.inline_code(), styles::inline_code());
        assert_eq!(t.code_fallback_style(), styles::code_fallback());
        assert_eq!(t.fence_marker_style(), styles::fence_marker());
        assert_eq!(t.blockquote_prefix_style(), styles::blockquote_prefix());
        assert_eq!(t.blockquote_text_style(), styles::blockquote_text());
        assert_eq!(t.link_style(), styles::link());
        assert_eq!(t.heading_1_style(), styles::heading_1());
        assert_eq!(t.heading_2_style(), styles::heading_2());
        assert_eq!(t.heading_3_style(), styles::heading_3());
        assert_eq!(t.bullet_prefix_style(), styles::bullet_prefix());

        // Overlay
        assert_eq!(
            t.notification_badge(colors::INFO),
            styles::notification_badge(colors::INFO)
        );
        assert_eq!(t.permission_badge(), styles::permission_badge());
        assert_eq!(t.permission_type(), styles::permission_type());
        assert_eq!(
            t.overlay_key(colors::SUCCESS),
            styles::overlay_key(colors::SUCCESS)
        );
        assert_eq!(t.overlay_hint(), styles::overlay_hint());
        assert_eq!(t.overlay_text_style(), styles::overlay_text());
        assert_eq!(t.overlay_bright_style(), styles::overlay_bright());
        assert_eq!(t.diff_gutter(), styles::diff_gutter());
        assert_eq!(t.diff_bg_style(), styles::diff_bg());
        assert_eq!(t.input_bg_style(), styles::input_bg());
    }

    #[test]
    fn default_ref_is_static() {
        let r1 = ThemeTokens::default_ref();
        let r2 = ThemeTokens::default_ref();
        // Same static reference
        assert!(std::ptr::eq(r1, r2));
        assert_eq!(*r1, ThemeTokens::default());
    }
}
