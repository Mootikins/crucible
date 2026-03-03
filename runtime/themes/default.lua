-- Default dark theme for Crucible TUI
-- Mirrors ThemeConfig::default_dark() exactly.
-- All color values match ThemeTokens::default_tokens().
--
-- Color formats:
--   "#rrggbb"  — hex RGB
--   "name"     — named terminal color (white, black, red, green, yellow, blue, magenta, cyan, gray, dark_gray)
--   { dark = "...", light = "..." }  — adaptive color (different for dark/light terminals)
--
-- This file is embedded via include_str!() in crucible-lua.

return {
  name = "default",
  is_dark = true,

  colors = {
    -- Core
    primary          = "cyan",          -- Primary accent (links, highlights, prompt)
    secondary        = "magenta",       -- Secondary accent (tool calls, metadata)
    background       = "#282c34",       -- Main background (input areas, panels)
    background_panel = "#23272f",       -- Panel/block background (code blocks)
    text             = "white",         -- Primary text
    text_muted       = "dark_gray",     -- Muted/secondary text
    text_dim         = "gray",          -- Dimmed text (timestamps, metadata)
    text_emphasized  = "cyan",          -- Emphasized text (accents)

    -- Semantic
    error   = "#f7768e",  -- Error indicator
    warning = "#e0af68",  -- Warning indicator
    success = "#9ece6a",  -- Success indicator
    info    = "#00ced1",  -- Info indicator

    -- Borders
    border         = "#282c34",  -- Default border (same as background)
    border_focused = "cyan",     -- Focused/active border
    border_dim     = "dark_gray", -- Dimmed border

    -- Chat roles
    user_message      = "green",    -- User message indicator
    assistant_message = "cyan",     -- Assistant message
    system_message    = "yellow",   -- System message

    -- Modes
    mode_normal = "green",   -- Normal mode badge background
    mode_insert = "cyan",    -- Insert mode badge background
    mode_plan   = "blue",    -- Plan mode badge background
    mode_auto   = "yellow",  -- Auto mode badge background

    -- Diff
    diff_added      = "#9ece6a",  -- Diff added line foreground
    diff_removed    = "#f7768e",  -- Diff removed line foreground
    diff_added_bg   = "#1c2028",  -- Diff added line background tint
    diff_removed_bg = "#1c2028",  -- Diff removed line background tint
    diff_context    = "#646e82",  -- Diff context line color

    -- Overlay
    popup_bg          = "#1e222a",  -- Popup/overlay background
    popup_selected_bg = "#323844",  -- Popup selected item background
    toast_bg          = "#282c34",  -- Toast notification background
    overlay_text      = "#c0caf5",  -- overlay/popup primary text (Rgb(192, 202, 245))

    -- Markdown rendering
    code_inline       = "yellow",     -- Inline code foreground
    code_fallback     = "green",      -- Code block fallback foreground
    fence_marker      = "dark_gray",  -- Fence marker (```) foreground
    bullet_prefix     = "dark_gray",  -- Bullet prefix foreground
    blockquote_prefix = "dark_gray",  -- Blockquote prefix (│) foreground
    blockquote_text   = "gray",       -- Blockquote text foreground
    link              = "blue",       -- Link foreground
    heading_1         = "cyan",       -- Heading level 1 foreground
    heading_2         = "blue",       -- Heading level 2 foreground
    heading_3         = "magenta",    -- Heading level 3 foreground
  },

  decorations = {
    border_style                 = "rounded",  -- Border drawing style
    message_user_indicator       = "▌",        -- Left-edge indicator for user messages
    message_assistant_indicator  = " ",        -- Left-edge indicator for assistant messages
    tool_pending_icon            = "●",        -- Icon for pending tool calls
    tool_success_icon            = "✓",        -- Icon for successful tool calls
    tool_error_icon              = "✖",        -- Icon for failed tool calls
    bullet_char                  = "•",        -- Bullet character for lists
    divider_char                 = "─",        -- Horizontal divider character
    check_char                   = "✓",        -- Checkmark character
    error_char                   = "✗",        -- Error/cross character
    separator_char               = "│",        -- Vertical separator character
    half_block_top               = "▀",        -- Half-block top (gradient effects)
    half_block_bottom            = "▄",        -- Half-block bottom (gradient effects)
  },

  icons = {
    check       = "✓",  -- Success/completion
    error       = "✖",  -- Failure/rejection
    warning     = "⚠",  -- Caution
    info        = "ℹ",  -- Informational
    loading     = "⟳",  -- Loading/spinner label
    arrow_right = "→",  -- Navigation/flow
  },

  spinner = "braille",  -- Spinner animation style: braille, braille_minidot, ascii, pulse, none

  layout = {
    status_bar_position = "bottom",  -- Status bar position: top, bottom, hidden
    message_spacing     = 1,         -- Blank lines between chat messages
    code_block_margin   = 0,         -- Top/bottom margin around code blocks
    input_max_lines     = 6,         -- Maximum visible lines for input field
  },
}
