#[cfg(test)]
use crate::ansi::visible_width;
use crate::cell_grid::{cells_to_string, CellGrid};
use crate::node::{Node, OverlayNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum OverlayAnchor {
    FromBottom(usize),
}

/// Draw `overlay` over `base` from column `start_col`. A blank cell of the
/// overlay lets the base show through.
///
/// Both lines are read by [`CellGrid`], the one ANSI and grapheme reader of
/// the renderer, so a wide or joined grapheme keeps its cells here too.
fn composite_line(base: &str, overlay: &str, start_col: usize, width: usize) -> String {
    let mut line = CellGrid::from_line(base, width);
    let mut layer = CellGrid::new(width, 1);
    layer.blit_line(overlay, start_col, 0);
    line.overlay_row_from(0, &layer, 0);
    cells_to_string(line.row(0))
}

pub fn extract_overlays(node: &Node) -> Vec<OverlayNode> {
    let mut overlays = Vec::new();
    collect_overlays(node, &mut overlays);
    overlays
}

fn collect_overlays(node: &Node, overlays: &mut Vec<OverlayNode>) {
    match node {
        Node::Overlay(overlay) => overlays.push(overlay.clone()),
        Node::Box(b) => b
            .children
            .iter()
            .for_each(|c| collect_overlays(c, overlays)),
        Node::Fragment(children) | Node::Slot { children, .. } => {
            children.iter().for_each(|c| collect_overlays(c, overlays))
        }
        Node::Action(a) => collect_overlays(&a.child, overlays),
        _ => {}
    }
}

pub fn filter_overlays(node: Node) -> Node {
    match node {
        Node::Overlay(_) => Node::Empty,
        Node::Box(mut b) => {
            b.children = b.children.into_iter().map(filter_overlays).collect();
            Node::Box(b)
        }
        Node::Fragment(children) => {
            Node::Fragment(children.into_iter().map(filter_overlays).collect())
        }
        Node::Slot { name, children } => Node::Slot {
            name,
            children: children.into_iter().map(filter_overlays).collect(),
        },
        other => other,
    }
}

#[derive(Debug, Clone)]
pub struct Overlay {
    pub lines: Vec<String>,
    pub anchor: OverlayAnchor,
}

impl Overlay {
    pub fn from_bottom(lines: Vec<String>, offset: usize) -> Self {
        Self {
            lines,
            anchor: OverlayAnchor::FromBottom(offset),
        }
    }
}

pub fn composite_overlays(base: &[String], overlays: &[Overlay], width: usize) -> Vec<String> {
    let mut result: Vec<String> = base
        .iter()
        .map(|l| crate::utils::truncate_to_width(l, width, false).into_owned())
        .collect();

    for overlay in overlays {
        match overlay.anchor {
            OverlayAnchor::FromBottom(preserve_bottom) => {
                let needed_height = overlay.lines.len() + preserve_bottom;
                if result.len() < needed_height {
                    let blank_line = " ".repeat(width);
                    let lines_to_add = needed_height - result.len();
                    let mut expanded = vec![blank_line; lines_to_add];
                    expanded.extend(result);
                    result = expanded;
                }

                let start_line = result
                    .len()
                    .saturating_sub(preserve_bottom + overlay.lines.len());

                for (i, overlay_line) in overlay.lines.iter().enumerate() {
                    let target_line = start_line + i;
                    if target_line < result.len().saturating_sub(preserve_bottom) {
                        result[target_line] =
                            composite_line(&result[target_line], overlay_line, 0, width);
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
fn pad_or_truncate(line: &str, width: usize) -> String {
    let vis_width = visible_width(line);
    match vis_width.cmp(&width) {
        std::cmp::Ordering::Greater => {
            crate::utils::truncate_to_width(line, width, false).into_owned()
        }
        std::cmp::Ordering::Less => format!("{}{}", line, " ".repeat(width - vis_width)),
        std::cmp::Ordering::Equal => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_with_no_overlays_returns_base() {
        let base = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];
        let result = composite_overlays(&base, &[], 80);
        assert_eq!(result, base);
    }

    #[test]
    fn overlay_from_bottom_replaces_correct_lines() {
        let base = vec![
            "chat1".to_string(),
            "chat2".to_string(),
            "chat3".to_string(),
            "input".to_string(),
            "status".to_string(),
        ];
        let popup_offset_from_bottom = 2;
        let overlay = Overlay::from_bottom(
            vec!["popup1".to_string(), "popup2".to_string()],
            popup_offset_from_bottom,
        );

        let result = composite_overlays(&base, &[overlay], 10);

        assert!(result[0].starts_with("chat1"));
        assert!(result[1].starts_with("popup1"));
        assert!(result[2].starts_with("popup2"));
        assert!(result[3].starts_with("input"));
        assert!(result[4].starts_with("status"));
    }

    #[test]
    fn overlay_at_bottom_edge() {
        let base = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];
        let overlay = Overlay::from_bottom(vec!["overlay".to_string()], 0);

        let result = composite_overlays(&base, &[overlay], 10);

        assert!(result[2].starts_with("overlay"));
    }

    #[test]
    fn multiple_overlays_composite_correctly() {
        let base = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let overlay1 = Overlay::from_bottom(vec!["X".to_string()], 2);
        let overlay2 = Overlay::from_bottom(vec!["Y".to_string()], 0);

        let result = composite_overlays(&base, &[overlay1, overlay2], 5);

        assert!(result[1].starts_with('X'));
        assert!(result[3].starts_with('Y'));
    }

    #[test]
    fn pad_or_truncate_pads_short_lines() {
        assert_eq!(pad_or_truncate("hello", 10), "hello     ");
        assert_eq!(pad_or_truncate("", 5), "     ");
    }

    #[test]
    fn pad_or_truncate_exact_width_unchanged() {
        assert_eq!(pad_or_truncate("12345", 5), "12345");
    }

    #[test]
    fn pad_or_truncate_truncates_long_lines() {
        assert_eq!(pad_or_truncate("hello world", 5), "hello");
        assert_eq!(pad_or_truncate("abcdefghij", 3), "abc");
    }

    /// A joined grapheme in an overlay keeps its cells: a ZWJ sequence is
    /// one wide glyph, not its code points one per cell.
    #[test]
    fn an_overlay_keeps_a_joined_grapheme_whole() {
        let coder = "\u{1F469}\u{200D}\u{1F4BB}";
        let line = composite_line("abcdef", &format!("{coder}x"), 0, 6);
        assert!(line.starts_with(&format!("{coder}x")), "{line:?}");
        assert_eq!(visible_width(&line), 6, "{line:?}");
    }

    #[test]
    fn truncate_preserves_ansi_codes() {
        let styled = "\x1b[31mred text\x1b[0m";
        let truncated = crate::utils::truncate_to_width(styled, 3, false);
        assert!(truncated.starts_with("\x1b[31m"));
        assert_eq!(visible_width(&truncated), 3);
    }

    #[test]
    fn truncate_handles_unicode_box_chars() {
        let border = "▄".repeat(100);
        let truncated = crate::utils::truncate_to_width(&border, 10, false);
        assert_eq!(visible_width(&truncated), 10);
    }

    #[test]
    fn composite_truncates_base_lines_exceeding_width() {
        let base = vec!["a".repeat(100)];
        let result = composite_overlays(&base, &[], 10);
        assert_eq!(result.len(), 1);
        assert_eq!(visible_width(&result[0]), 10);
    }

    #[test]
    fn composite_truncates_base_lines_with_overlays() {
        let base = vec!["a".repeat(100), "b".repeat(100), "c".repeat(100)];
        let overlay = Overlay::from_bottom(vec!["overlay".to_string()], 0);
        let result = composite_overlays(&base, &[overlay], 10);

        assert_eq!(visible_width(&result[0]), 10);
        assert_eq!(visible_width(&result[1]), 10);
        assert!(result[2].starts_with("overlay"));
    }
}
