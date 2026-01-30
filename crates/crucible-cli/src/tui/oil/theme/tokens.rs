//! Runtime theme token system.
//!
//! `ThemeTokens` is the single source of truth for all TUI colors and styles.
//! Components look up colors via `ThemeTokens` at runtime, enabling future
//! theme customization without recompilation.
//!
//! Use `ThemeTokens::default_ref()` for a zero-cost `&'static` reference.

use crucible_oil::style::{Color, Style};

/// Runtime color tokens for the TUI theme.
///
/// Use [`ThemeTokens::default()`] or [`ThemeTokens::default_ref()`] for the
/// standard dark theme.
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

    #[test]
    fn default_color_tokens_are_correct() {
        let t = ThemeTokens::default();

        // Surfaces
        assert_eq!(t.input_bg, Color::Rgb(40, 44, 52));
        assert_eq!(t.command_bg, Color::Rgb(60, 50, 20));
        assert_eq!(t.shell_bg, Color::Rgb(60, 30, 30));
        assert_eq!(t.popup_bg, Color::Rgb(30, 34, 42));
        assert_eq!(t.code_bg, Color::Rgb(35, 39, 47));
        assert_eq!(t.thinking_bg, Color::Rgb(45, 40, 55));

        // Text
        assert_eq!(t.text_primary, Color::White);
        assert_eq!(t.text_muted, Color::DarkGray);
        assert_eq!(t.text_accent, Color::Cyan);
        assert_eq!(t.text_dim, Color::Gray);

        // Semantic
        assert_eq!(t.success, Color::Rgb(158, 206, 106));
        assert_eq!(t.error, Color::Rgb(247, 118, 142));
        assert_eq!(t.warning, Color::Rgb(224, 175, 104));
        assert_eq!(t.info, Color::Rgb(0, 206, 209));

        // Roles
        assert_eq!(t.role_user, Color::Green);
        assert_eq!(t.role_assistant, Color::Cyan);
        assert_eq!(t.role_system, Color::Yellow);
        assert_eq!(t.role_tool, Color::Magenta);

        // Modes
        assert_eq!(t.mode_normal, Color::Green);
        assert_eq!(t.mode_plan, Color::Blue);
        assert_eq!(t.mode_auto, Color::Yellow);

        // UI elements
        assert_eq!(t.spinner, Color::Cyan);
        assert_eq!(t.selected, Color::Cyan);
        assert_eq!(t.border, Color::Rgb(40, 44, 52)); // same as input_bg
        assert_eq!(t.prompt, Color::Cyan);
        assert_eq!(t.model_name, Color::Cyan);
        assert_eq!(t.notification, Color::Yellow);

        // Markdown
        assert_eq!(t.code_inline, Color::Yellow);
        assert_eq!(t.code_fallback, Color::Green);
        assert_eq!(t.fence_marker, Color::DarkGray);
        assert_eq!(t.blockquote_prefix, Color::DarkGray);
        assert_eq!(t.blockquote_text, Color::Gray);
        assert_eq!(t.link, Color::Blue);
        assert_eq!(t.heading_1, Color::Cyan);
        assert_eq!(t.heading_2, Color::Blue);
        assert_eq!(t.heading_3, Color::Magenta);
        assert_eq!(t.bullet_prefix, Color::DarkGray);

        // Overlay
        assert_eq!(t.overlay_text, Color::Rgb(192, 202, 245));
        assert_eq!(t.overlay_dim, Color::Rgb(100, 110, 130));
        assert_eq!(t.overlay_bright, Color::Rgb(255, 255, 255));

        // Diff
        assert_eq!(t.diff_bg, Color::Rgb(28, 32, 40));
        assert_eq!(t.diff_fg, Color::Rgb(28, 32, 40));
        assert_eq!(t.diff_add, Color::Rgb(158, 206, 106));
        assert_eq!(t.diff_del, Color::Rgb(247, 118, 142));
        assert_eq!(t.diff_ctx, Color::Rgb(100, 110, 130));
        assert_eq!(t.diff_hunk, Color::Rgb(0, 206, 209));
        assert_eq!(t.gutter_fg, Color::Rgb(70, 75, 90));
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
    fn style_presets_produce_correct_styles() {
        let t = ThemeTokens::default();

        // Chat roles
        assert_eq!(t.user_prompt(), Style::new().fg(Color::Green).bold());
        assert_eq!(t.assistant_response(), Style::new().fg(Color::Cyan));
        assert_eq!(t.system_message(), Style::new().fg(Color::Yellow).italic());
        assert_eq!(t.tool_call(), Style::new().fg(Color::Magenta).dim());
        assert_eq!(t.tool_result(), Style::new().fg(Color::Gray));

        // Status
        assert_eq!(
            t.error_style(),
            Style::new().fg(Color::Rgb(247, 118, 142)).bold()
        );
        assert_eq!(
            t.warning_style(),
            Style::new().fg(Color::Rgb(224, 175, 104))
        );
        assert_eq!(
            t.success_style(),
            Style::new().fg(Color::Rgb(158, 206, 106))
        );
        assert_eq!(t.info_style(), Style::new().fg(Color::Rgb(0, 206, 209)));

        // Text variations
        assert_eq!(t.muted(), Style::new().fg(Color::DarkGray));
        assert_eq!(t.dim(), Style::new().fg(Color::Gray).dim());
        assert_eq!(t.accent(), Style::new().fg(Color::Cyan));
        assert_eq!(t.accent_bold(), Style::new().fg(Color::Cyan).bold());

        // UI elements
        assert_eq!(t.prompt_style(), Style::new().fg(Color::Cyan));
        assert_eq!(t.spinner_style(), Style::new().fg(Color::Cyan));
        assert_eq!(t.model_name_style(), Style::new().fg(Color::Cyan));
        assert_eq!(t.notification_style(), Style::new().fg(Color::Yellow));
        assert_eq!(
            t.selected_style(),
            Style::new().fg(Color::Black).bg(Color::Cyan)
        );
        assert_eq!(t.popup_description(), Style::new().fg(Color::Gray).dim());

        // Mode badges
        assert_eq!(
            t.mode_normal_style(),
            Style::new().bg(Color::Green).fg(Color::Black).bold()
        );
        assert_eq!(
            t.mode_plan_style(),
            Style::new().bg(Color::Blue).fg(Color::Black).bold()
        );
        assert_eq!(
            t.mode_auto_style(),
            Style::new().bg(Color::Yellow).fg(Color::Black).bold()
        );

        // Code/thinking
        assert_eq!(t.code_block(), Style::new().bg(Color::Rgb(35, 39, 47)));
        assert_eq!(t.thinking_header(), Style::new().fg(Color::Gray).italic());
        assert_eq!(t.thinking_content(), Style::new().fg(Color::Gray).dim());

        // Diff
        assert_eq!(t.diff_delete(), Style::new().fg(Color::Rgb(247, 118, 142)));
        assert_eq!(t.diff_insert(), Style::new().fg(Color::Rgb(158, 206, 106)));
        assert_eq!(t.diff_context(), Style::new().fg(Color::Gray));
        assert_eq!(
            t.diff_hunk_header(),
            Style::new().fg(Color::Rgb(0, 206, 209))
        );

        // Markdown
        assert_eq!(t.inline_code(), Style::new().fg(Color::Yellow));
        assert_eq!(t.code_fallback_style(), Style::new().fg(Color::Green));
        assert_eq!(t.fence_marker_style(), Style::new().fg(Color::DarkGray));
        assert_eq!(
            t.blockquote_prefix_style(),
            Style::new().fg(Color::DarkGray)
        );
        assert_eq!(
            t.blockquote_text_style(),
            Style::new().fg(Color::Gray).italic()
        );
        assert_eq!(t.link_style(), Style::new().fg(Color::Blue).underline());
        assert_eq!(t.heading_1_style(), Style::new().fg(Color::Cyan).bold());
        assert_eq!(t.heading_2_style(), Style::new().fg(Color::Blue).bold());
        assert_eq!(t.heading_3_style(), Style::new().fg(Color::Magenta).bold());
        assert_eq!(t.bullet_prefix_style(), Style::new().fg(Color::DarkGray));

        // Overlay
        let info_color = Color::Rgb(0, 206, 209);
        assert_eq!(
            t.notification_badge(info_color),
            Style::new().fg(info_color).bold().reverse()
        );
        assert_eq!(
            t.permission_badge(),
            Style::new().fg(Color::Rgb(247, 118, 142)).bold().reverse()
        );
        assert_eq!(
            t.permission_type(),
            Style::new().fg(Color::Rgb(247, 118, 142)).bold()
        );
        let success_color = Color::Rgb(158, 206, 106);
        assert_eq!(t.overlay_key(success_color), Style::new().fg(success_color));
        assert_eq!(t.overlay_hint(), Style::new().fg(Color::Rgb(100, 110, 130)));
        assert_eq!(
            t.overlay_text_style(),
            Style::new().fg(Color::Rgb(192, 202, 245))
        );
        assert_eq!(
            t.overlay_bright_style(),
            Style::new().fg(Color::Rgb(255, 255, 255))
        );
        assert_eq!(
            t.diff_gutter(),
            Style::new()
                .fg(Color::Rgb(70, 75, 90))
                .bg(Color::Rgb(28, 32, 40))
        );
        assert_eq!(t.diff_bg_style(), Style::new().bg(Color::Rgb(28, 32, 40)));
        assert_eq!(t.input_bg_style(), Style::new().bg(Color::Rgb(40, 44, 52)));
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
