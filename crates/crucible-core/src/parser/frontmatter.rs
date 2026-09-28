//! The one scan that splits a note into its frontmatter block and its body.
//!
//! The note parser, task files and workflows all read frontmatter through
//! [`split_frontmatter`]. It returns slices of the text it was given, so a
//! caller can compute byte offsets into the file from the body it gets back:
//! the parser's block spans depend on that.
//!
//! A YAML block is found by [`crate::note_frontmatter::split_fences`], the
//! scan that the frontmatter writer uses, so a note that the writer sees as
//! having frontmatter is also read that way. The writer does not write TOML,
//! so the `+++` scan is here.

use super::types::FrontmatterFormat;

/// A note's frontmatter block and the body that follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontmatterSplit<'a> {
    /// The text between the two delimiter lines.
    pub raw: &'a str,
    pub format: FrontmatterFormat,
    /// Everything after the closing delimiter line.
    pub body: &'a str,
}

impl FrontmatterSplit<'_> {
    /// The byte offset of the body in the text that was split.
    pub fn body_offset(&self, content: &str) -> usize {
        content.len() - self.body.len()
    }
}

/// Split `content` into its frontmatter block and its body, or `None` when
/// the note has no frontmatter.
///
/// A YAML block opens with a `---` line, after an optional byte order mark,
/// and closes at the next `---` or `...` line. A TOML block opens with a
/// `+++` line at byte 0 and closes at the next `+++` line. Lines may end in
/// `\n` or `\r\n`. A closing line at the end of the file, with no line ending
/// after it, closes the block and leaves an empty body. An opening line with
/// no closing one is not frontmatter.
pub fn split_frontmatter(content: &str) -> Option<FrontmatterSplit<'_>> {
    use crate::note_frontmatter::{split_fences, FrontmatterError};
    match split_fences(content) {
        Ok(split) => split.header.map(|header| FrontmatterSplit {
            raw: without_last_line_ending(header.yaml),
            format: FrontmatterFormat::Yaml,
            body: split.body,
        }),
        Err(FrontmatterError::Toml) => split_at(content, "+++", FrontmatterFormat::Toml),
        Err(_) => None,
    }
}

/// `lines` without the line ending of its last line.
fn without_last_line_ending(lines: &str) -> &str {
    let lines = lines.strip_suffix('\n').unwrap_or(lines);
    lines.strip_suffix('\r').unwrap_or(lines)
}

fn split_at<'a>(
    content: &'a str,
    delimiter: &str,
    format: FrontmatterFormat,
) -> Option<FrontmatterSplit<'a>> {
    let rest = content.strip_prefix(delimiter)?;
    let start = delimiter.len()
        + if rest.starts_with("\r\n") {
            2
        } else if rest.starts_with('\n') {
            1
        } else {
            return None;
        };

    // Each closing line: `\n` + delimiter + a line ending or the end of the
    // text. The first one after the opening line wins.
    let mut search = start - 1;
    while let Some(found) = content[search..].find(&format!("\n{delimiter}")) {
        let line_start = search + found;
        let after = line_start + 1 + delimiter.len();
        let tail = &content[after..];
        let skip = if tail.starts_with("\r\n") {
            Some(2)
        } else if tail.starts_with('\n') {
            Some(1)
        } else if tail.is_empty() {
            Some(0)
        } else {
            None
        };
        if let Some(skip) = skip {
            // A `\r` before the `\n` belongs to the line ending, not the block.
            let raw_end = if content[..line_start].ends_with('\r') {
                line_start - 1
            } else {
                line_start
            };
            return Some(FrontmatterSplit {
                raw: &content[start..raw_end.max(start)],
                format,
                body: &content[after + skip..],
            });
        }
        search = line_start + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(content: &str) -> Option<(&str, FrontmatterFormat, &str)> {
        split_frontmatter(content).map(|s| (s.raw, s.format, s.body))
    }

    #[test]
    fn yaml_and_toml_blocks_split_from_the_body() {
        assert_eq!(
            split("---\ntitle: Test\n---\n# Content"),
            Some(("title: Test", FrontmatterFormat::Yaml, "# Content"))
        );
        assert_eq!(
            split("+++\ntitle = \"Test\"\n+++\nContent"),
            Some(("title = \"Test\"", FrontmatterFormat::Toml, "Content"))
        );
    }

    #[test]
    fn windows_line_endings_split_without_a_stray_carriage_return() {
        assert_eq!(
            split("---\r\ntitle: Test\r\n---\r\nBody"),
            Some(("title: Test", FrontmatterFormat::Yaml, "Body"))
        );
    }

    #[test]
    fn a_closing_delimiter_at_the_end_of_the_file_leaves_an_empty_body() {
        assert_eq!(
            split("---\ntitle: Test\n---"),
            Some(("title: Test", FrontmatterFormat::Yaml, ""))
        );
        assert_eq!(
            split("---\n---\nBody"),
            Some(("", FrontmatterFormat::Yaml, "Body"))
        );
    }

    #[test]
    fn text_that_is_not_a_closed_block_is_not_frontmatter() {
        for content in [
            "Just content",
            "",
            "---\ntitle: Test\nno closing line",
            "--- not a delimiter line\n---\n",
            " ---\ntitle: Test\n---\n",
        ] {
            assert_eq!(split(content), None, "{content:?}");
        }
    }

    #[test]
    fn a_longer_dash_line_does_not_close_the_block() {
        assert_eq!(
            split("---\na: 1\n----\nb: 2\n---\nBody"),
            Some(("a: 1\n----\nb: 2", FrontmatterFormat::Yaml, "Body"))
        );
    }

    /// The writer's rules: a byte order mark before the block, and `...` as
    /// the closing line of a YAML block.
    #[test]
    fn a_yaml_block_follows_the_rules_of_the_frontmatter_writer() {
        assert_eq!(
            split("\u{feff}---\ntitle: T\n---\nBody"),
            Some(("title: T", FrontmatterFormat::Yaml, "Body"))
        );
        assert_eq!(
            split("---\ntitle: T\n...\nBody"),
            Some(("title: T", FrontmatterFormat::Yaml, "Body"))
        );
        let text = "---\ntitle: T\n...\nBody";
        assert_eq!(
            crate::note_frontmatter::split_fences(text).unwrap().body,
            split_frontmatter(text).unwrap().body
        );
    }

    #[test]
    fn the_body_offset_is_a_byte_offset_into_the_file() {
        let content = "---\ntitle: T\n---\nBody";
        let split = split_frontmatter(content).unwrap();
        assert_eq!(&content[split.body_offset(content)..], "Body");
    }
}
