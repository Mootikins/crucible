//! Content truncation for session logging
//!
//! Large tool results are truncated to keep JSONL files manageable.
//! The truncation preserves the head and tail of content for context.

/// Default threshold for truncation (4KB)
pub const DEFAULT_TRUNCATE_THRESHOLD: usize = 4 * 1024;

/// Head size to preserve when truncating (3KB)
const HEAD_SIZE: usize = 3 * 1024;

/// Tail size to preserve when truncating (512 bytes)
const TAIL_SIZE: usize = 512;

/// Result of truncation operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncateResult {
    /// The (possibly truncated) content
    pub content: String,
    /// Whether truncation occurred
    pub truncated: bool,
    /// Original size in bytes
    pub original_size: usize,
}

impl TruncateResult {
    pub fn unchanged(content: String) -> Self {
        let size = content.len();
        Self {
            content,
            truncated: false,
            original_size: size,
        }
    }
}

/// Truncate content for logging, preserving head and tail for context.
///
/// If content is under the threshold, returns it unchanged.
/// Otherwise, keeps the first HEAD_SIZE bytes and last TAIL_SIZE bytes,
/// with a marker showing how much was removed.
pub fn truncate_for_log(content: &str, threshold: usize) -> TruncateResult {
    let original_size = content.len();

    if original_size <= threshold {
        return TruncateResult {
            content: content.to_string(),
            truncated: false,
            original_size,
        };
    }

    let head_end = find_char_boundary(content, HEAD_SIZE);
    let tail_start = find_char_boundary_rev(content, original_size.saturating_sub(TAIL_SIZE));

    if tail_start <= head_end {
        let truncated_content = format!(
            "{}\n\n[...{} bytes truncated...]",
            &content[..head_end],
            original_size - head_end
        );
        return TruncateResult {
            content: truncated_content,
            truncated: true,
            original_size,
        };
    }

    let omitted = tail_start - head_end;
    let truncated_content = format!(
        "{}\n\n[...{} bytes truncated...]\n\n{}",
        &content[..head_end],
        omitted,
        &content[tail_start..]
    );

    TruncateResult {
        content: truncated_content,
        truncated: true,
        original_size,
    }
}

fn find_char_boundary(s: &str, mut pos: usize) -> usize {
    pos = pos.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn find_char_boundary_rev(s: &str, mut pos: usize) -> usize {
    pos = pos.min(s.len());
    while pos < s.len() && !s.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_content_unchanged() {
        let content = "Hello, world!";
        let result = truncate_for_log(content, DEFAULT_TRUNCATE_THRESHOLD);

        assert!(!result.truncated);
        assert_eq!(result.content, content);
        assert_eq!(result.original_size, content.len());
    }

    #[test]
    fn exactly_at_threshold_unchanged() {
        let content = "x".repeat(DEFAULT_TRUNCATE_THRESHOLD);
        let result = truncate_for_log(&content, DEFAULT_TRUNCATE_THRESHOLD);

        assert!(!result.truncated);
        assert_eq!(result.content, content);
    }

    #[test]
    fn over_threshold_truncated() {
        let content = "x".repeat(10_000);
        let result = truncate_for_log(&content, DEFAULT_TRUNCATE_THRESHOLD);

        assert!(result.truncated);
        assert_eq!(result.original_size, 10_000);
        assert!(result.content.len() < 10_000);
        assert!(result.content.contains("[..."));
        assert!(result.content.contains("bytes truncated...]"));
    }

    #[test]
    fn preserves_head_and_tail() {
        let head = "HEAD_MARKER_".repeat(100);
        let middle = "m".repeat(10_000);
        let tail = "_TAIL_MARKER".repeat(20);
        let content = format!("{}{}{}", head, middle, tail);

        let result = truncate_for_log(&content, DEFAULT_TRUNCATE_THRESHOLD);

        assert!(result.truncated);
        assert!(result.content.starts_with("HEAD_MARKER_"));
        assert!(result.content.ends_with("_TAIL_MARKER"));
    }

    #[test]
    fn handles_utf8_boundaries() {
        let content = "🎉".repeat(2000);
        let result = truncate_for_log(&content, DEFAULT_TRUNCATE_THRESHOLD);

        assert!(result.truncated);
        assert!(result.content.is_char_boundary(0));
        for (i, _) in result.content.char_indices() {
            assert!(result.content.is_char_boundary(i));
        }
    }

    #[test]
    fn small_threshold_triggers_truncation() {
        let content = "Hello, this is a test message that should be truncated.";
        let result = truncate_for_log(content, 20);

        assert!(result.truncated);
        assert!(result.content.contains("[..."));
        assert_eq!(result.original_size, content.len());
    }

    #[test]
    fn very_small_threshold_head_only() {
        let content = "x".repeat(1000);
        let result = truncate_for_log(&content, 100);

        assert!(result.truncated);
        assert!(result.content.contains("[..."));
    }
}
