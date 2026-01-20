use crate::ansi::visible_width;
use crate::node::{Node, OverlayNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAnchor {
    FromBottom(usize),
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
        Node::Fragment(children) => children.iter().for_each(|c| collect_overlays(c, overlays)),
        Node::Static(s) => s
            .children
            .iter()
            .for_each(|c| collect_overlays(c, overlays)),
        Node::Focusable(f) => collect_overlays(&f.child, overlays),
        Node::ErrorBoundary(b) => collect_overlays(&b.child, overlays),
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
        Node::Static(mut s) => {
            s.children = s.children.into_iter().map(filter_overlays).collect();
            Node::Static(s)
        }
        Node::Focusable(mut f) => {
            f.child = Box::new(filter_overlays(*f.child));
            Node::Focusable(f)
        }
        Node::ErrorBoundary(mut b) => {
            b.child = Box::new(filter_overlays(*b.child));
            b.fallback = Box::new(filter_overlays(*b.fallback));
            Node::ErrorBoundary(b)
        }
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
    let mut result: Vec<String> = base.iter().map(|l| truncate_to_width(l, width)).collect();

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
                        result[target_line] = pad_or_truncate(overlay_line, width);
                    }
                }
            }
        }
    }

    result
}

fn pad_or_truncate(line: &str, width: usize) -> String {
    let vis_width = visible_width(line);
    match vis_width.cmp(&width) {
        std::cmp::Ordering::Greater => truncate_to_width(line, width),
        std::cmp::Ordering::Less => format!("{}{}", line, " ".repeat(width - vis_width)),
        std::cmp::Ordering::Equal => line.to_string(),
    }
}

/// Truncate a string with ANSI codes to a maximum visible width.
/// ANSI escape sequences are preserved but visible characters are limited.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    let mut result = String::new();
    let mut current_width = 0;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ANSI escape sequence: copy entirely (doesn't count toward visible width)
            result.push(c);
            if chars.peek() == Some(&'[') {
                result.push(chars.next().unwrap());
                while let Some(&next) = chars.peek() {
                    result.push(chars.next().unwrap());
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            let char_width = UnicodeWidthChar::width(c).unwrap_or(1);
            if current_width + char_width > max_width {
                break;
            }
            result.push(c);
            current_width += char_width;
        }
    }

    result
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

        assert_eq!(result[2], "overlay   ");
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

        assert_eq!(result[1], "X    ");
        assert_eq!(result[3], "Y    ");
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

    #[test]
    fn truncate_preserves_ansi_codes() {
        let styled = "\x1b[31mred text\x1b[0m";
        let truncated = truncate_to_width(styled, 3);
        assert!(truncated.starts_with("\x1b[31m"));
        assert_eq!(visible_width(&truncated), 3);
    }

    #[test]
    fn truncate_handles_unicode_box_chars() {
        let border = "▄".repeat(100);
        let truncated = truncate_to_width(&border, 10);
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
