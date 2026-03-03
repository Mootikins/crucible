-- OpenCode-inspired dark theme for Crucible TUI
-- An alternative theme proving the Lua theme system works with different values.
--
-- Color formats:
--   "#rrggbb"  — hex RGB
--   "name"     — named terminal color (white, black, red, green, yellow, blue, magenta, cyan, gray, dark_gray)
--   { dark = "...", light = "..." }  — adaptive color (different for dark/light terminals)
--
-- This file is embedded via include_str!() in crucible-lua.

return {
  name = "opencode",
  is_dark = true,

  colors = {
    -- Core
    primary          = "#fab283",    -- Orange-gold (signature OpenCode color)
    secondary        = "#5c9cf5",    -- Blue
    background       = "#212121",    -- Dark gray (not as dark as default)
    background_panel = "#2a2a2a",    -- Slightly lighter panel
    command_bg       = "#2a2010",    -- Dark amber tint for command mode
    shell_bg         = "#2a1010",    -- Dark red tint for shell mode
    text             = "#e0e0e0",    -- Light gray text
    text_muted       = "#6a6a6a",    -- Muted gray
    text_dim         = "#4a4a4a",    -- Dimmer gray
    text_emphasized  = "#fab283",    -- Same as primary

    -- Semantic
    error   = "#f44747",  -- Red
    warning = "#ffcc00",  -- Yellow
    success = "#4ec9b0",  -- Teal green
    info    = "#9cdcfe",  -- Light blue

    -- Borders
    border         = "#3a3a3a",  -- Subtle border
    border_focused = "#fab283",  -- Orange-gold focused border
    border_dim     = "#2a2a2a",  -- Very dim border

    -- Chat roles
    user_message      = "#fab283",  -- Orange-gold for user
    assistant_message = "#9cdcfe",  -- Light blue for assistant
    system_message    = "#6a6a6a",  -- Muted for system

    -- Modes
    mode_normal = "#fab283",  -- Orange-gold normal mode
    mode_insert = "#4ec9b0",  -- Teal insert mode
    mode_plan   = "#5c9cf5",  -- Blue plan mode
    mode_auto   = "#ffcc00",  -- Yellow auto mode

    -- Diff
    diff_added      = "#4ec9b0",  -- Teal
    diff_removed    = "#f44747",  -- Red
    diff_added_bg   = "#1a2a28",  -- Dark teal tint
    diff_removed_bg = "#2a1a1a",  -- Dark red tint
    diff_context    = "#4a4a4a",  -- Dim gray

    -- Overlay
    popup_bg          = "#1a1a1a",  -- Very dark popup
    popup_selected_bg = "#2d2d2d",  -- Slightly lighter selected
    toast_bg          = "#212121",  -- Same as background
    overlay_text      = "#e0e0e0",  -- Same as text
    overlay_bright    = "#ffffff",  -- White

    -- Markdown rendering
    code_inline       = "#fab283",    -- Orange-gold inline code
    code_fallback     = "#4ec9b0",    -- Teal code block fallback
    fence_marker      = "#6a6a6a",    -- Muted fence markers
    bullet_prefix     = "#6a6a6a",    -- Muted bullet prefix
    blockquote_prefix = "#6a6a6a",    -- Muted blockquote prefix
    blockquote_text   = "#4a4a4a",    -- Dim blockquote text
    link              = "#5c9cf5",    -- Blue links
    heading_1         = "#fab283",    -- Orange-gold heading 1
    heading_2         = "#5c9cf5",    -- Blue heading 2
    heading_3         = "#4ec9b0",    -- Teal heading 3
  },

  decorations = {
    border_style                 = "sharp",   -- Key visual difference from default's "rounded"
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
