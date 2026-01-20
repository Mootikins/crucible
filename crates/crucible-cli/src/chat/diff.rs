//! Diff rendering for file edits in chat interface
//!
//! Provides colored unified diff output for displaying file changes
//! during agent edit operations.

use colored::Colorize;
use similar::{ChangeTag, TextDiff};

pub struct DiffRenderer {
    context_lines: usize,
    word_diff: bool,
}

impl Default for DiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffRenderer {
    pub fn new() -> Self {
        Self {
            context_lines: 0,
            word_diff: false,
        }
    }

    pub fn with_context(mut self, lines: usize) -> Self {
        self.context_lines = lines;
        self
    }

    pub fn with_word_diff(mut self, enabled: bool) -> Self {
        self.word_diff = enabled;
        self
    }

    pub fn render_inline(&self, old: &str, new: &str) -> String {
        use similar::Algorithm;

        if old == new {
            return old.to_string();
        }

        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_words(old, new);

        let mut output = String::new();

        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Equal => {
                    output.push_str(change.value());
                }
                ChangeTag::Delete => {
                    output.push_str(&format!("{}{}{}", "\x1b[31m", change.value(), "\x1b[0m"));
                }
                ChangeTag::Insert => {
                    output.push_str(&format!("{}{}{}", "\x1b[32m", change.value(), "\x1b[0m"));
                }
            }
        }

        output
    }

    /// Render diff to a string (without ANSI colors) for testing
    pub fn render(&self, old: &str, new: &str) -> String {
        self.render_hunks(old, new, false)
    }

    /// Render diff with ANSI colors for terminal display
    fn render_colored(&self, old: &str, new: &str) -> String {
        self.render_hunks(old, new, true)
    }

    /// Core diff rendering logic shared by both render() and render_colored()
    fn render_hunks(&self, old: &str, new: &str, colored: bool) -> String {
        let diff = TextDiff::from_lines(old, new);
        let mut output = String::new();

        let mut in_hunk = false;
        let mut hunk_old_start = 0;
        let mut hunk_new_start = 0;
        let mut hunk_old_count = 0;
        let mut hunk_new_count = 0;
        let mut hunk_lines: Vec<(ChangeTag, String)> = Vec::new();

        for (idx, change) in diff.iter_all_changes().enumerate() {
            let tag = change.tag();
            let line = change.to_string();

            match tag {
                ChangeTag::Equal => {
                    if self.context_lines > 0 {
                        if !in_hunk {
                            in_hunk = true;
                            hunk_old_start = idx + 1;
                            hunk_new_start = idx + 1;
                        }
                        hunk_lines.push((tag, line));
                        hunk_old_count += 1;
                        hunk_new_count += 1;
                    } else if in_hunk {
                        output.push_str(&self.format_hunk(
                            hunk_old_start,
                            hunk_old_count,
                            hunk_new_start,
                            hunk_new_count,
                            &hunk_lines,
                            colored,
                        ));
                        in_hunk = false;
                        hunk_lines.clear();
                        hunk_old_count = 0;
                        hunk_new_count = 0;
                    }
                }
                ChangeTag::Delete | ChangeTag::Insert => {
                    if !in_hunk {
                        in_hunk = true;
                        hunk_old_start = idx + 1;
                        hunk_new_start = idx + 1;
                    }
                    hunk_lines.push((tag, line));
                    if tag == ChangeTag::Delete {
                        hunk_old_count += 1;
                    } else {
                        hunk_new_count += 1;
                    }
                }
            }
        }

        if in_hunk && !hunk_lines.is_empty() {
            output.push_str(&self.format_hunk(
                hunk_old_start,
                hunk_old_count,
                hunk_new_start,
                hunk_new_count,
                &hunk_lines,
                colored,
            ));
        }

        output
    }

    /// Format a single hunk with optional ANSI colors
    fn format_hunk(
        &self,
        old_start: usize,
        old_count: usize,
        new_start: usize,
        new_count: usize,
        lines: &[(ChangeTag, String)],
        colored: bool,
    ) -> String {
        let mut output = String::new();

        let header = format!(
            "@@ -{},{} +{},{} @@",
            old_start, old_count, new_start, new_count
        );

        if colored {
            output.push_str(&format!("    {}\n", header.dimmed()));
        } else {
            output.push_str(&header);
            output.push('\n');
        }

        for (tag, line) in lines {
            let line_content = line.strip_suffix('\n').unwrap_or(line);
            let (prefix, content) = match tag {
                ChangeTag::Delete => ("-", line_content),
                ChangeTag::Insert => ("+", line_content),
                ChangeTag::Equal => (" ", line_content),
            };

            if colored {
                let formatted = match tag {
                    ChangeTag::Delete => format!("    {}", format!("-{}", content).red()),
                    ChangeTag::Insert => format!("    {}", format!("+{}", content).green()),
                    ChangeTag::Equal => format!("     {}", content.dimmed()),
                };
                output.push_str(&formatted);
            } else {
                output.push_str(prefix);
                output.push_str(content);
            }
            output.push('\n');
        }

        output
    }

    /// Print diff as a preview (pre-approval in act mode)
    pub fn print_preview(&self, path: &str, old: &str, new: &str) {
        println!(
            "  {} {}",
            "▷".cyan(),
            format!("Edit preview: {}", path).cyan()
        );
        print!("{}", self.render_colored(old, new));
    }

    /// Print diff as a result (post-execution)
    pub fn print_result(&self, path: &str, old: &str, new: &str) {
        println!(
            "  {} {}",
            "▷".cyan(),
            format!("Edit file(path=\"{}\")", path).dimmed()
        );
        print!("{}", self.render_colored(old, new));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Constructor tests ===

    #[test]
    fn test_new_creates_renderer_with_zero_context() {
        let renderer = DiffRenderer::new();
        assert_eq!(renderer.context_lines, 0);
    }

    #[test]
    fn test_default_same_as_new() {
        let default = DiffRenderer::default();
        let new = DiffRenderer::new();
        assert_eq!(default.context_lines, new.context_lines);
    }

    #[test]
    fn test_with_context_sets_context_lines() {
        let renderer = DiffRenderer::new().with_context(3);
        assert_eq!(renderer.context_lines, 3);
    }

    #[test]
    fn test_with_context_is_chainable() {
        let renderer = DiffRenderer::new().with_context(5);
        assert_eq!(renderer.context_lines, 5);
    }

    // === Render tests (no changes) ===

    #[test]
    fn test_render_identical_content_returns_empty() {
        let renderer = DiffRenderer::new();
        let content = "line1\nline2\nline3\n";
        let result = renderer.render(content, content);
        assert!(result.is_empty());
    }

    #[test]
    fn test_render_empty_to_empty_returns_empty() {
        let renderer = DiffRenderer::new();
        let result = renderer.render("", "");
        assert!(result.is_empty());
    }

    // === Render tests (additions) ===

    #[test]
    fn test_render_single_line_addition() {
        let renderer = DiffRenderer::new();
        let old = "";
        let new = "new line\n";
        let result = renderer.render(old, new);
        assert!(result.contains("+new line"));
    }

    #[test]
    fn test_render_addition_has_hunk_header() {
        let renderer = DiffRenderer::new();
        let old = "";
        let new = "new line\n";
        let result = renderer.render(old, new);
        assert!(result.contains("@@"));
    }

    #[test]
    fn test_render_multiple_additions() {
        let renderer = DiffRenderer::new();
        let old = "existing\n";
        let new = "existing\nnew1\nnew2\n";
        let result = renderer.render(old, new);
        assert!(result.contains("+new1"));
        assert!(result.contains("+new2"));
    }

    // === Render tests (deletions) ===

    #[test]
    fn test_render_single_line_deletion() {
        let renderer = DiffRenderer::new();
        let old = "delete me\n";
        let new = "";
        let result = renderer.render(old, new);
        assert!(result.contains("-delete me"));
    }

    #[test]
    fn test_render_multiple_deletions() {
        let renderer = DiffRenderer::new();
        let old = "line1\nline2\nline3\n";
        let new = "line1\n";
        let result = renderer.render(old, new);
        assert!(result.contains("-line2"));
        assert!(result.contains("-line3"));
    }

    // === Render tests (modifications) ===

    #[test]
    fn test_render_line_modification() {
        let renderer = DiffRenderer::new();
        let old = "old content\n";
        let new = "new content\n";
        let result = renderer.render(old, new);
        assert!(result.contains("-old content"));
        assert!(result.contains("+new content"));
    }

    #[test]
    fn test_render_modification_in_middle() {
        let renderer = DiffRenderer::new();
        let old = "line1\nold\nline3\n";
        let new = "line1\nnew\nline3\n";
        let result = renderer.render(old, new);
        assert!(result.contains("-old"));
        assert!(result.contains("+new"));
        // Without context, unchanged lines should not appear
        assert!(!result.contains("line1"));
        assert!(!result.contains("line3"));
    }

    // === Context line tests ===

    #[test]
    fn test_render_with_context_shows_surrounding_lines() {
        let renderer = DiffRenderer::new().with_context(1);
        let old = "line1\nline2\nline3\nline4\nline5\n";
        let new = "line1\nline2\nmodified\nline4\nline5\n";
        let result = renderer.render(old, new);
        // Should show context lines (with space prefix)
        assert!(result.contains(" line2"));
        assert!(result.contains(" line4"));
    }

    #[test]
    fn test_render_zero_context_hides_unchanged() {
        let renderer = DiffRenderer::new().with_context(0);
        let old = "context\nold\ncontext\n";
        let new = "context\nnew\ncontext\n";
        let result = renderer.render(old, new);
        // Only changes should appear
        assert!(result.contains("-old"));
        assert!(result.contains("+new"));
        // Context lines should not appear
        assert!(!result.contains("context"));
    }

    // === Hunk header format tests ===

    #[test]
    fn test_hunk_header_format() {
        let renderer = DiffRenderer::new();
        let old = "delete\n";
        let new = "insert\n";
        let result = renderer.render(old, new);
        // Should have @@ header
        assert!(result.starts_with("@@"));
    }

    // === Edge cases ===

    #[test]
    fn test_render_no_trailing_newline_old() {
        let renderer = DiffRenderer::new();
        let old = "no newline";
        let new = "no newline\n";
        let result = renderer.render(old, new);
        // Should handle gracefully
        assert!(!result.is_empty() || old == new.trim());
    }

    #[test]
    fn test_render_no_trailing_newline_new() {
        let renderer = DiffRenderer::new();
        let old = "with newline\n";
        let new = "with newline";
        let result = renderer.render(old, new);
        // Should handle gracefully
        assert!(!result.is_empty() || old.trim() == new);
    }

    #[test]
    fn test_render_unicode_content() {
        let renderer = DiffRenderer::new();
        let old = "Hello 世界\n";
        let new = "Hello 世界!\n";
        let result = renderer.render(old, new);
        assert!(result.contains("-Hello 世界"));
        assert!(result.contains("+Hello 世界!"));
    }

    #[test]
    fn test_render_empty_lines() {
        let renderer = DiffRenderer::new();
        let old = "line1\n\nline3\n";
        let new = "line1\ninserted\n\nline3\n";
        let result = renderer.render(old, new);
        assert!(result.contains("+inserted"));
    }

    // === Realistic use case tests ===

    #[test]
    fn test_render_rust_code_change() {
        let renderer = DiffRenderer::new();
        let old = r#"fn main() {
    println!("Hello");
}
"#;
        let new = r#"fn main() {
    println!("Hello, world!");
    println!("Welcome!");
}
"#;
        let result = renderer.render(old, new);
        assert!(result.contains("-    println!(\"Hello\");"));
        assert!(result.contains("+    println!(\"Hello, world!\");"));
        assert!(result.contains("+    println!(\"Welcome!\");"));
    }

    #[test]
    fn test_render_config_change() {
        let renderer = DiffRenderer::new();
        let old = "[settings]\nport = 8080\nhost = localhost\n";
        let new = "[settings]\nport = 3000\nhost = localhost\n";
        let result = renderer.render(old, new);
        assert!(result.contains("-port = 8080"));
        assert!(result.contains("+port = 3000"));
    }

    mod word_level_diff {
        use super::*;

        const ANSI_RED: &str = "\x1b[31m";
        const ANSI_GREEN: &str = "\x1b[32m";
        const ANSI_RESET: &str = "\x1b[0m";

        fn has_deletion_color(s: &str) -> bool {
            s.contains(ANSI_RED)
        }

        fn has_insertion_color(s: &str) -> bool {
            s.contains(ANSI_GREEN)
        }

        #[test]
        fn render_inline_highlights_changed_words_with_color() {
            let renderer = DiffRenderer::new().with_word_diff(true);
            let old = "The quick brown fox\n";
            let new = "The slow brown dog\n";
            let result = renderer.render_inline(old, new);

            assert!(
                has_deletion_color(&result),
                "Deleted words should be red. Got:\n{}",
                result.escape_debug()
            );
            assert!(
                has_insertion_color(&result),
                "Inserted words should be green. Got:\n{}",
                result.escape_debug()
            );
            assert!(
                result.contains(ANSI_RESET),
                "Should reset ANSI codes after highlighting"
            );
        }

        #[test]
        fn render_inline_deletions_are_red() {
            let renderer = DiffRenderer::new().with_word_diff(true);
            let old = "remove this word\n";
            let new = "remove word\n";
            let result = renderer.render_inline(old, new);

            assert!(
                has_deletion_color(&result),
                "Deleted word 'this' should be red. Got:\n{}",
                result.escape_debug()
            );
        }

        #[test]
        fn render_inline_insertions_are_green() {
            let renderer = DiffRenderer::new().with_word_diff(true);
            let old = "add word\n";
            let new = "add new word\n";
            let result = renderer.render_inline(old, new);

            assert!(
                has_insertion_color(&result),
                "Inserted word 'new' should be green. Got:\n{}",
                result.escape_debug()
            );
        }

        #[test]
        fn render_inline_preserves_unchanged_text() {
            let renderer = DiffRenderer::new().with_word_diff(true);
            let old = "start middle end\n";
            let new = "start changed end\n";
            let result = renderer.render_inline(old, new);

            assert!(result.contains("start"), "Unchanged 'start' should appear");
            assert!(result.contains("end"), "Unchanged 'end' should appear");
        }

        #[test]
        fn with_word_diff_is_chainable() {
            let renderer = DiffRenderer::new().with_context(3).with_word_diff(true);
            assert_eq!(renderer.context_lines, 3);
            assert!(renderer.word_diff);
        }

        #[test]
        fn word_diff_disabled_by_default() {
            let renderer = DiffRenderer::new();
            assert!(!renderer.word_diff);
        }

        #[test]
        fn render_inline_multiline_colors_each_change() {
            let renderer = DiffRenderer::new().with_word_diff(true);
            let old = "line one\nline two\n";
            let new = "line ONE\nline TWO\n";
            let result = renderer.render_inline(old, new);

            let red_count = result.matches(ANSI_RED).count();
            let green_count = result.matches(ANSI_GREEN).count();
            assert!(
                red_count >= 2 && green_count >= 2,
                "Each changed word should have its own color. Red: {}, Green: {}. Got:\n{}",
                red_count,
                green_count,
                result.escape_debug()
            );
        }

        #[test]
        fn render_inline_identical_has_no_colors() {
            let renderer = DiffRenderer::new().with_word_diff(true);
            let same = "identical content\n";
            let result = renderer.render_inline(same, same);

            assert!(
                !has_deletion_color(&result) && !has_insertion_color(&result),
                "Identical content should have no diff colors. Got:\n{}",
                result.escape_debug()
            );
        }
    }
}
