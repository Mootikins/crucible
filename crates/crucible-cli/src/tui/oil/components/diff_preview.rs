//! Inline diff preview component for permission prompts.
//!
//! Displays file diffs above the input area when requesting write/create/delete permissions.

use crate::formatting::{HighlightedLine, SyntaxHighlighter};
use crate::tui::oil::diff::{diff_to_node, diff_to_node_width};
use crate::tui::oil::node::{col, row, styled, Node};
use crate::tui::oil::style::{Color, Style};
use crate::tui::oil::theme::ThemeTokens;

/// Maximum number of lines to display before truncating.
const MAX_LINES: usize = 500;

/// Renders a diff preview for file operations.
///
/// # Arguments
///
/// * `file_path` - Path to the file being modified
/// * `action` - The action type: "write", "create", or "delete"
/// * `old_content` - Previous file content (None for new files)
/// * `new_content` - New file content (None for deletions)
/// * `collapsed` - If true, only show the header
///
/// # Returns
///
/// A `Node` representing the diff preview UI.
pub fn render_diff_preview(
    file_path: &str,
    action: &str,
    old_content: Option<&str>,
    new_content: Option<&str>,
    collapsed: bool,
) -> Node {
    render_diff_preview_width(file_path, action, old_content, new_content, collapsed, None)
}

pub fn render_diff_preview_width(
    file_path: &str,
    action: &str,
    old_content: Option<&str>,
    new_content: Option<&str>,
    collapsed: bool,
    max_width: Option<usize>,
) -> Node {
    let header = render_header(file_path, action);

    if collapsed {
        return header;
    }

    let extension = extract_extension(file_path);

    let diff_content = match (action, old_content, new_content) {
        ("create", None, Some(content)) | ("write", None, Some(content)) => {
            render_all_lines_styled(content, true, extension.as_deref())
        }
        ("delete", Some(content), None) | ("delete", Some(content), _) => {
            render_all_lines_styled(content, false, extension.as_deref())
        }
        ("write", Some(old), Some(new)) => render_modification_diff(old, new, max_width),
        _ => Node::Empty,
    };

    col([header, diff_content])
}

/// Extracts the file extension from a path.
fn extract_extension(file_path: &str) -> Option<String> {
    std::path::Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
}

/// Checks if a file extension indicates a binary file that shouldn't be highlighted.
fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "ico"
            | "webp"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "mp3"
            | "mp4"
            | "avi"
            | "mkv"
            | "mov"
            | "wav"
            | "flac"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "eot"
    )
}

fn render_header(file_path: &str, action: &str) -> Node {
    let label = match action {
        "create" => "[new file]",
        "delete" => "[deleting file]",
        "write" => "[write]",
        _ => "[unknown]",
    };

    let theme = ThemeTokens::default_ref();
    let label_style = match action {
        "create" => theme.diff_insert(),
        "delete" => theme.diff_delete(),
        _ => theme.info_style(),
    };

    row([
        styled(format!("{} ", label), label_style),
        styled(file_path.to_string(), theme.accent()),
    ])
}

fn render_all_lines_styled(content: &str, is_insert: bool, extension: Option<&str>) -> Node {
    let theme = ThemeTokens::default_ref();
    let diff_bg = if is_insert {
        theme.success
    } else {
        theme.error
    };
    let prefix = if is_insert { "+" } else { "-" };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let truncated = total_lines > MAX_LINES;
    let display_lines = if truncated {
        &lines[..MAX_LINES]
    } else {
        &lines[..]
    };

    let should_highlight = extension
        .map(|ext| !is_binary_extension(ext) && SyntaxHighlighter::supports_language(ext))
        .unwrap_or(false);

    let mut nodes: Vec<Node> = if should_highlight {
        let ext = extension.unwrap();
        let highlighter = SyntaxHighlighter::new();
        let display_content = display_lines.join("\n");
        let highlighted_lines = highlighter.highlight(&display_content, ext);

        highlighted_lines
            .into_iter()
            .map(|line| render_highlighted_diff_line(&line, prefix, diff_bg))
            .collect()
    } else {
        let fallback_style = Style::new().fg(diff_bg);
        display_lines
            .iter()
            .map(|line| styled(format!("{}{}", prefix, line), fallback_style))
            .collect()
    };

    if truncated {
        let remaining = total_lines - MAX_LINES;
        nodes.push(styled(
            format!("... {} more lines", remaining),
            ThemeTokens::default_ref().muted(),
        ));
    }

    col(nodes)
}

fn render_highlighted_diff_line(line: &HighlightedLine, prefix: &str, diff_bg: Color) -> Node {
    if line.spans.is_empty() {
        return styled(prefix.to_string(), Style::new().fg(diff_bg));
    }

    let mut children = vec![styled(prefix.to_string(), Style::new().fg(diff_bg))];

    for span in &line.spans {
        let combined_style = if span.style.fg.is_some() {
            span.style
        } else {
            Style::new().fg(diff_bg)
        };
        children.push(styled(span.text.clone(), combined_style));
    }

    row(children)
}

fn render_modification_diff(old: &str, new: &str, max_width: Option<usize>) -> Node {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let total_lines = old_lines.len().max(new_lines.len());

    if total_lines > MAX_LINES {
        let truncated_old = truncate_content(old, MAX_LINES);
        let truncated_new = truncate_content(new, MAX_LINES);
        let diff_node = diff_to_node_width(&truncated_old, &truncated_new, 3, max_width);

        let remaining = total_lines.saturating_sub(MAX_LINES);
        col([
            diff_node,
            styled(
                format!("... {} more lines", remaining),
                ThemeTokens::default_ref().muted(),
            ),
        ])
    } else {
        diff_to_node_width(old, new, 3, max_width)
    }
}

fn truncate_content(content: &str, max_lines: usize) -> String {
    content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_string;
    use insta::assert_snapshot;

    #[test]
    fn new_file_shows_all_green() {
        let node = render_diff_preview("test.rs", "create", None, Some("line1\nline2"), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[new file]"));
        assert!(output.contains("test.rs"));
        assert!(output.contains("+"), "should contain + prefix");
        assert!(output.contains("line1"), "should contain line1");
        assert!(output.contains("line2"), "should contain line2");
        assert_snapshot!(output);
    }

    #[test]
    fn delete_file_shows_all_red() {
        let node = render_diff_preview("test.rs", "delete", Some("line1\nline2"), None, false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[deleting file]"));
        assert!(output.contains("test.rs"));
        assert!(output.contains("-"), "should contain - prefix");
        assert!(output.contains("line1"), "should contain line1");
        assert!(output.contains("line2"), "should contain line2");
        assert_snapshot!(output);
    }

    #[test]
    fn modification_shows_diff() {
        let node = render_diff_preview(
            "test.rs",
            "write",
            Some("old line"),
            Some("new line"),
            false,
        );
        let output = render_to_string(&node, 80);

        assert!(output.contains("[write]"));
        assert!(output.contains("test.rs"));
        assert!(output.contains("-old line"));
        assert!(output.contains("+new line"));
        assert_snapshot!(output);
    }

    #[test]
    fn collapsed_shows_only_header() {
        let node = render_diff_preview(
            "test.rs",
            "write",
            Some("old content"),
            Some("new content"),
            true,
        );
        let output = render_to_string(&node, 80);

        assert!(output.contains("[write]"));
        assert!(output.contains("test.rs"));
        assert!(!output.contains("-old"));
        assert!(!output.contains("+new"));
        assert_snapshot!(output);
    }

    #[test]
    fn truncates_large_files() {
        let large_content: String = (0..600).map(|i| format!("line {}\n", i)).collect();
        let node = render_diff_preview("large.rs", "create", None, Some(&large_content), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("... 100 more lines"));
        assert_snapshot!(output);
    }

    #[test]
    fn write_with_no_old_content_treated_as_new() {
        let node = render_diff_preview("new.rs", "write", None, Some("content"), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("+"), "should contain + prefix");
        assert!(output.contains("content"), "should contain content");
        assert_snapshot!(output);
    }

    #[test]
    fn rust_file_gets_syntax_highlighting() {
        let rust_code = "fn main() {\n    println!(\"Hello\");\n}";
        let node = render_diff_preview("test.rs", "create", None, Some(rust_code), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("fn"), "should contain fn keyword");
        assert!(output.contains("main"), "should contain main");
        assert!(output.contains("println"), "should contain println");
        assert_snapshot!(output);
    }

    #[test]
    fn typescript_file_gets_syntax_highlighting() {
        let ts_code = "const x: number = 42;";
        let node = render_diff_preview("test.ts", "create", None, Some(ts_code), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("const"), "should contain const keyword");
        assert!(output.contains("42"), "should contain number");
        assert_snapshot!(output);
    }

    #[test]
    fn unknown_extension_gets_plain_text() {
        let content = "some plain text";
        let node = render_diff_preview("test.xyz", "create", None, Some(content), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("+"), "should contain + prefix");
        assert!(output.contains("some plain text"), "should contain content");
        assert_snapshot!(output);
    }

    #[test]
    fn binary_extension_not_highlighted() {
        let content = "binary content";
        let node = render_diff_preview("test.png", "create", None, Some(content), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("+"), "should contain + prefix");
        assert!(output.contains("binary content"), "should contain content");
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_new_rust_file() {
        let rust_code = r#"fn main() {
    println!("Hello, world!");
}
"#;
        let node = render_diff_preview("src/main.rs", "create", None, Some(rust_code), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[new file]"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("+"));
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_modification() {
        let old_content = "fn greet() {\n    println!(\"Hi\");\n}";
        let new_content = "fn greet() {\n    println!(\"Hello, world!\");\n}";
        let node = render_diff_preview(
            "src/lib.rs",
            "write",
            Some(old_content),
            Some(new_content),
            false,
        );
        let output = render_to_string(&node, 80);

        assert!(output.contains("[write]"));
        assert!(output.contains("src/lib.rs"));
        assert!(output.contains("-"));
        assert!(output.contains("+"));
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_deletion() {
        let content = "fn deprecated() {\n    // This function is no longer used\n}";
        let node = render_diff_preview("src/old.rs", "delete", Some(content), None, false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[deleting file]"));
        assert!(output.contains("src/old.rs"));
        assert!(output.contains("-"));
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_collapsed() {
        let old_content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let new_content = "line 1\nline 2 modified\nline 3\nline 4\nline 5";
        let node = render_diff_preview(
            "config.toml",
            "write",
            Some(old_content),
            Some(new_content),
            true,
        );
        let output = render_to_string(&node, 80);

        assert!(output.contains("[write]"));
        assert!(output.contains("config.toml"));
        assert!(!output.contains("-"));
        assert!(!output.contains("+"));
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_truncated() {
        let large_content: String = (0..600).map(|i| format!("line {}\n", i)).collect();
        let node =
            render_diff_preview("large_file.rs", "create", None, Some(&large_content), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[new file]"));
        assert!(output.contains("large_file.rs"));
        assert!(output.contains("... 100 more lines"));
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_typescript() {
        let ts_code = r#"interface User {
    id: number;
    name: string;
}

const user: User = { id: 1, name: "Alice" };
"#;
        let node = render_diff_preview("src/types.ts", "create", None, Some(ts_code), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[new file]"));
        assert!(output.contains("src/types.ts"));
        assert!(output.contains("+"));
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_diff_preview_unknown_extension() {
        let content = "This is a custom format file\nWith multiple lines\nNo syntax highlighting";
        let node = render_diff_preview("data.custom", "create", None, Some(content), false);
        let output = render_to_string(&node, 80);

        assert!(output.contains("[new file]"));
        assert!(output.contains("data.custom"));
        assert!(output.contains("+"));
        assert_snapshot!(output);
    }
}
