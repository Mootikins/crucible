//! Shared text rendering helpers for both hand-written and Taffy-based renderers.

use crate::ansi::apply_style;
use crate::style::Style;
use textwrap::{wrap, Options, WordSplitter};

/// Wraps text to a given width and applies a style to each line.
///
/// This helper is used by both `render.rs` (hand-written flex pipeline) and
/// `tree_render.rs` (Taffy pipeline) to ensure consistent text wrapping and styling.
///
/// # Arguments
/// - `content`: The text to wrap and style (should not contain embedded newlines)
/// - `style`: The style to apply to each line
/// - `width`: The target width for wrapping (in characters)
///
/// # Returns
/// A vector of styled lines. Each line is:
/// 1. Wrapped to fit within `width` characters
/// 2. Styled with `apply_style()`
/// 3. NOT padded (caller is responsible for padding if needed)
///
/// If `content` is empty or `width` is 0, returns an empty vector.
pub(crate) fn wrap_and_style(content: &str, style: &Style, width: usize) -> Vec<String> {
    if content.is_empty() || width == 0 {
        return Vec::new();
    }

    // Wrap text to the target width
    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
    let wrapped: Vec<_> = wrap(content, options);

    // Style each line (no padding)
    wrapped
        .into_iter()
        .map(|line| apply_style(&line, style))
        .collect()
}

/// Wraps text to a given width, applies a style, and pads each line to fill the width.
///
/// This is used by tree_render.rs where background colors need to extend across the full width.
///
/// # Arguments
/// - `content`: The text to wrap and style
/// - `style`: The style to apply to each line
/// - `width`: The target width for wrapping and padding (in characters)
///
/// # Returns
/// A vector of styled, padded lines. Each line is:
/// 1. Wrapped to fit within `width` characters
/// 2. Styled with `apply_style()`
/// 3. Right-padded with spaces to exactly `width` characters
///
/// If `content` is empty or `width` is 0, returns an empty vector.
/// If wrapping produces no lines (e.g., all-whitespace input), returns one full-width line of spaces.
pub(crate) fn wrap_and_style_padded(content: &str, style: &Style, width: usize) -> Vec<String> {
    if content.is_empty() || width == 0 {
        return Vec::new();
    }

    // Wrap text to the target width
    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
    let wrapped: Vec<_> = wrap(content, options);

    // If wrapping produced no lines (e.g., all-whitespace input), return one full-width line of spaces
    if wrapped.is_empty() {
        return vec![apply_style(&" ".repeat(width), style)];
    }

    // Style each line and pad to fill the width
    wrapped
        .into_iter()
        .map(|line| {
            let visual_len = line.chars().count();
            let padded = if visual_len < width {
                format!("{}{}", line, " ".repeat(width - visual_len))
            } else {
                line.into_owned()
            };
            apply_style(&padded, style)
        })
        .collect()
}

/// Selects the current spinner frame character from the given frames array.
///
/// This helper is used by both `render.rs` (hand-written flex pipeline) and
/// `tree_render.rs` (Taffy pipeline) to ensure consistent spinner frame selection.
///
/// # Arguments
/// - `frame`: The current frame index (will be wrapped using modulo)
/// - `frames`: Optional custom frames array; if None, uses default SPINNER_FRAMES
///
/// # Returns
/// The character for the current frame, or '⠋' if frames is empty.
pub(crate) fn select_spinner_frame(frame: usize, frames: Option<&'static [char]>) -> char {
    use crate::node::SPINNER_FRAMES;
    let spinner_frames = frames.unwrap_or(SPINNER_FRAMES);
    spinner_frames
        .get(frame % spinner_frames.len())
        .copied()
        .unwrap_or('⠋')
}
