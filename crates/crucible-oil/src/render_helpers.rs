//! Shared text rendering helpers for both hand-written and Taffy-based renderers.

use crate::ansi::{apply_style, visible_width};

use crate::style::Style;
use textwrap::{wrap, Options, WordSplitter};

/// Wraps text to a given width, applies a style, and pads each line to fill
/// the width. Used by tree_render.rs where background colors need to extend
/// across the full width.
///
/// Renders at most `max_rows` lines (pass `usize::MAX` for no clamp). When
/// the wrapped text exceeds `max_rows`, the last visible line ends in an
/// ellipsis so truncation is visible instead of silently bleeding past the
/// laid-out rect (e.g., a shrunk status-bar span whose rect is 1 row tall).
pub(crate) fn wrap_and_style_padded_clamped(
    content: &str,
    style: &Style,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if content.is_empty() || width == 0 || max_rows == 0 {
        return Vec::new();
    }

    // Wrap text to the target width
    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
    let mut wrapped: Vec<_> = wrap(content, options);

    // If wrapping produced no lines (e.g., all-whitespace input), return one full-width line of spaces
    if wrapped.is_empty() {
        return vec![apply_style(&" ".repeat(width), style)];
    }

    if wrapped.len() > max_rows {
        wrapped.truncate(max_rows);
        let last = wrapped
            .last_mut()
            .expect("max_rows > 0 guarantees a last line");
        let mut clamped = last.to_string();
        if visible_width(&clamped) >= width {
            clamped = crate::utils::truncate_to_width(&clamped, width.saturating_sub(1), false)
                .into_owned();
        }
        clamped.push('\u{2026}');
        *last = clamped.into();
    }

    // Style each line and pad to fill the width. Padding cells inherit
    // the style — this is intentional: a styled `text("foo")` with a bg
    // color forms a colored bar across the laid-out width (e.g., the
    // input bar, mode bar, user-message highlight). The CellGrid's
    // compact path preserves these styled-space cells via the
    // `!c.style.is_empty()` rule.
    wrapped
        .into_iter()
        .map(|line| {
            let visual_len = visible_width(&line);
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
    if spinner_frames.is_empty() {
        return '⠋';
    }
    spinner_frames
        .get(frame % spinner_frames.len())
        .copied()
        .unwrap_or('⠋')
}

/// Truncates text to a maximum character count, appending an ellipsis if truncated.
///
/// # Arguments
/// - `input`: The text to truncate
/// - `max_chars`: The maximum number of characters (including the ellipsis)
///
/// # Returns
/// The input text if it fits within `max_chars`, or the first `max_chars - 1` characters
/// followed by an ellipsis (U+2026) if truncation is needed.
pub(crate) fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if input.chars().count() > max_chars && max_chars > 1 {
        let s: String = input.chars().take(max_chars - 1).collect();
        format!("{}\u{2026}", s)
    } else {
        input.to_string()
    }
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
    let label_text = truncate_with_ellipsis(label, max_label_width);
    line.push_str(&label_text);

    let label_width = visible_width(&line);

    if let Some(desc) = description {
        let available = width.saturating_sub(label_width + 3);
        if available > 10 {
            let truncated = truncate_with_ellipsis(desc, available);

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
