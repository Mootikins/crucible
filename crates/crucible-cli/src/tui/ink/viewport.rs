//! Viewport utilities for stable line clamping and padding.

/// Clamp lines to show only the first `max` lines.
pub fn clamp_lines_top(lines: &[String], max: usize) -> Vec<String> {
    if max == 0 {
        return Vec::new();
    }
    lines.iter().take(max).cloned().collect()
}

/// Clamp lines to show only the last `max` lines.
pub fn clamp_lines_bottom(lines: &[String], max: usize) -> Vec<String> {
    if max == 0 {
        return Vec::new();
    }
    let skip = lines.len().saturating_sub(max);
    lines.iter().skip(skip).cloned().collect()
}

/// Pad lines to exactly `height` by adding empty strings.
/// If lines exceed height, truncates from top (keeps bottom).
pub fn pad_lines_to(lines: &mut Vec<String>, height: usize) {
    if lines.len() > height {
        let skip = lines.len() - height;
        *lines = lines.drain(skip..).collect();
    }
    while lines.len() < height {
        lines.push(String::new());
    }
}

/// Ensure lines have at least `min_height`, padding with empty strings if needed.
pub fn ensure_min_height(lines: &mut Vec<String>, min_height: usize) {
    while lines.len() < min_height {
        lines.push(String::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_lines_top_under_limit() {
        let lines: Vec<String> = vec!["a".into(), "b".into()];
        let result = clamp_lines_top(&lines, 5);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn clamp_lines_top_at_limit() {
        let lines: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let result = clamp_lines_top(&lines, 3);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn clamp_lines_top_over_limit() {
        let lines: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let result = clamp_lines_top(&lines, 2);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn clamp_lines_top_zero() {
        let lines: Vec<String> = vec!["a".into()];
        let result = clamp_lines_top(&lines, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn clamp_lines_bottom_under_limit() {
        let lines: Vec<String> = vec!["a".into(), "b".into()];
        let result = clamp_lines_bottom(&lines, 5);
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn clamp_lines_bottom_at_limit() {
        let lines: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let result = clamp_lines_bottom(&lines, 3);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn clamp_lines_bottom_over_limit() {
        let lines: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let result = clamp_lines_bottom(&lines, 2);
        assert_eq!(result, vec!["c", "d"]);
    }

    #[test]
    fn clamp_lines_bottom_zero() {
        let lines: Vec<String> = vec!["a".into()];
        let result = clamp_lines_bottom(&lines, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn pad_lines_to_under_height() {
        let mut lines: Vec<String> = vec!["a".into(), "b".into()];
        pad_lines_to(&mut lines, 4);
        assert_eq!(lines, vec!["a", "b", "", ""]);
    }

    #[test]
    fn pad_lines_to_at_height() {
        let mut lines: Vec<String> = vec!["a".into(), "b".into()];
        pad_lines_to(&mut lines, 2);
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn pad_lines_to_over_height_truncates_top() {
        let mut lines: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        pad_lines_to(&mut lines, 2);
        assert_eq!(lines, vec!["c", "d"]);
    }

    #[test]
    fn pad_lines_to_zero() {
        let mut lines: Vec<String> = vec!["a".into(), "b".into()];
        pad_lines_to(&mut lines, 0);
        assert!(lines.is_empty());
    }

    #[test]
    fn ensure_min_height_pads() {
        let mut lines: Vec<String> = vec!["a".into()];
        ensure_min_height(&mut lines, 3);
        assert_eq!(lines, vec!["a", "", ""]);
    }

    #[test]
    fn ensure_min_height_no_op_when_sufficient() {
        let mut lines: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        ensure_min_height(&mut lines, 2);
        assert_eq!(lines, vec!["a", "b", "c"]);
    }
}
