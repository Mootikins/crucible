//! Shared text rendering helpers for both hand-written and Taffy-based renderers.

use crate::ansi::{apply_style, visible_width};

use crate::style::Style;
use textwrap::{wrap, Options, WordSplitter};

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

/// Formats a single popup item line (unstyled) with the standard layout:
/// `[indicator] [kind] [label]  [description]` padded to `width`.
///
/// Used by both `render.rs` (hand-written flex pipeline) and `tree_render.rs`
/// (Taffy pipeline) to ensure consistent popup item formatting.
///
/// # Arguments
/// - `is_selected`: Whether this item is currently selected (shows ▸ indicator)
/// - `kind`: Optional kind indicator (e.g., "file", "cmd")
/// - `label`: The item label text
/// - `description`: Optional description text shown after the label
/// - `width`: The available width for the formatted line
///
/// # Returns
/// An unstyled string containing the formatted popup item line, padded to fill
/// the width. The caller is responsible for applying styling.
pub(crate) fn format_popup_item_line(
    is_selected: bool,
    kind: Option<&str>,
    label: &str,
    description: Option<&str>,
    width: usize,
) -> String {
    let mut line = String::new();
    line.push(' ');

    if is_selected {
        line.push_str("▸ ");
    } else {
        line.push_str("  ");
    }

    if let Some(kind) = kind {
        line.push_str(kind);
        line.push(' ');
    }

    let prefix_width = visible_width(&line);
    let max_label_width = width.saturating_sub(prefix_width + 2);
    let label_text = if label.chars().count() > max_label_width && max_label_width > 4 {
        let s: String = label.chars().take(max_label_width - 1).collect();
        format!("{}\u{2026}", s)
    } else {
        label.to_string()
    };
    line.push_str(&label_text);

    let label_width = visible_width(&line);

    if let Some(desc) = description {
        let available = width.saturating_sub(label_width + 3);
        if available > 10 {
            let truncated = if desc.chars().count() > available {
                let s: String = desc.chars().take(available - 1).collect();
                format!("{}\u{2026}", s)
            } else {
                desc.to_string()
            };

            line.push_str("  ");
            line.push_str(&truncated);

            let after_desc_width = label_width + 2 + visible_width(&truncated);
            let padding = width.saturating_sub(after_desc_width);
            line.push_str(&" ".repeat(padding));
        } else {
            let padding = width.saturating_sub(label_width);
            line.push_str(&" ".repeat(padding));
        }
    } else {
        let padding = width.saturating_sub(label_width);
            line.push_str(&" ".repeat(padding));
    }

    line
}
