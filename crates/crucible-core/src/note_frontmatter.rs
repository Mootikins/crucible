//! Text-preserving YAML frontmatter for Markdown notes.
//!
//! A note is `BOM? + header? + body`. The header is a `---` line, YAML lines,
//! and a closing `---` or `...` line. This module splits a note into those
//! parts without a change to any byte. It also sets or deletes one top-level
//! key through a line splice, so that comments, quoting, flow lists, anchors
//! and key order stay as the author wrote them. A re-serialization of the full
//! mapping would lose all of them.

use serde_yaml::{Mapping, Value};

/// The reasons that a note's frontmatter cannot be read or changed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FrontmatterError {
    /// The note starts with a `+++` line (TOML frontmatter).
    #[error("Bases property writes require YAML frontmatter")]
    Toml,
    /// The note starts with `---` but no later line closes the header.
    #[error("Unclosed YAML frontmatter")]
    Unclosed,
    /// The header does not parse as a YAML mapping, or a splice gave a wrong result.
    #[error("Invalid YAML frontmatter: {0}")]
    Invalid(String),
    /// The property name is empty or holds a line break.
    #[error("Invalid property name")]
    InvalidKey,
}

/// The split of one note. Byte-exact: `bom + header.raw + body == text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split<'a> {
    /// `"\r\n"` when the first line ending of the note is CRLF, else `"\n"`.
    pub newline: &'static str,
    /// `"\u{feff}"` when the note starts with a byte order mark, else `""`.
    pub bom: &'a str,
    /// `None` when the note has no frontmatter.
    pub header: Option<Header<'a>>,
    /// All text after the closing fence line.
    pub body: &'a str,
}

/// The frontmatter block of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header<'a> {
    /// The lines between the fences, with their line endings.
    pub yaml: &'a str,
    /// The opening fence line through the closing fence line, inclusive.
    pub raw: &'a str,
}

const BOM: &str = "\u{feff}";

/// The line ending style of `text`, from its first line ending.
#[must_use]
pub fn newline_of(text: &str) -> &'static str {
    match text.find('\n') {
        Some(at) if text[..at].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// A line without its `\n` or `\r\n` terminator.
fn content_of(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// Find the fences of the frontmatter, but do not parse the YAML.
///
/// Use this function when a caller must remove a header that does not parse.
///
/// # Errors
///
/// [`FrontmatterError::Toml`] for a `+++` first line,
/// [`FrontmatterError::Unclosed`] for a `---` first line without a closing fence.
pub fn split_fences(text: &str) -> Result<Split<'_>, FrontmatterError> {
    let newline = newline_of(text);
    let (bom, rest) = match text.strip_prefix(BOM) {
        Some(rest) => (BOM, rest),
        None => ("", text),
    };
    let no_header = Split {
        newline,
        bom,
        header: None,
        body: rest,
    };
    let mut lines = rest.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Ok(no_header);
    };
    match content_of(first) {
        "+++" => return Err(FrontmatterError::Toml),
        "---" => {}
        _ => return Ok(no_header),
    }
    let mut offset = first.len();
    for line in lines {
        let end = offset + line.len();
        if matches!(content_of(line), "---" | "...") {
            return Ok(Split {
                newline,
                bom,
                header: Some(Header {
                    yaml: &rest[first.len()..offset],
                    raw: &rest[..end],
                }),
                body: &rest[end..],
            });
        }
        offset = end;
    }
    Err(FrontmatterError::Unclosed)
}

/// Parse header YAML as a mapping. Blank or comment-only YAML is an empty mapping.
fn parse_mapping(yaml: &str) -> Result<Mapping, FrontmatterError> {
    match serde_yaml::from_str::<Value>(yaml) {
        Ok(Value::Mapping(mapping)) => Ok(mapping),
        Ok(Value::Null) => Ok(Mapping::new()),
        Ok(_) => Err(FrontmatterError::Invalid(
            "the header is not a mapping".to_string(),
        )),
        Err(error) => Err(FrontmatterError::Invalid(error.to_string())),
    }
}

/// Split a note into BOM, header and body, and check that the header is a YAML mapping.
///
/// # Errors
///
/// The errors of [`split_fences`], and [`FrontmatterError::Invalid`] when the
/// header is not blank and does not parse as a mapping.
pub fn split_yaml_frontmatter(text: &str) -> Result<Split<'_>, FrontmatterError> {
    let split = split_fences(text)?;
    if let Some(header) = split.header {
        parse_mapping(header.yaml)?;
    }
    Ok(split)
}

/// The parsed header mapping. The mapping is empty when there is no header or it is empty.
///
/// # Errors
///
/// The errors of [`split_yaml_frontmatter`].
pub fn frontmatter_mapping(text: &str) -> Result<Mapping, FrontmatterError> {
    match split_fences(text)?.header {
        Some(header) => parse_mapping(header.yaml),
        None => Ok(Mapping::new()),
    }
}

/// Set (`Some`) or delete (`None`) one top-level key.
///
/// Returns `Ok(None)` when the text would not change: the key already holds an
/// equal value, or a delete names an absent key. All other bytes of the note
/// stay the same. A JSON `null` writes `key:` with no value.
///
/// # Errors
///
/// [`FrontmatterError::InvalidKey`] for an empty key or a key with a line
/// break, the errors of [`split_yaml_frontmatter`], and
/// [`FrontmatterError::Invalid`] when the splice cannot give the requested
/// mapping (for example, a key that comes from a merge key).
pub fn set_frontmatter_key(
    text: &str,
    key: &str,
    value: Option<&serde_json::Value>,
) -> Result<Option<String>, FrontmatterError> {
    if key.is_empty() || key.contains(['\n', '\r']) {
        return Err(FrontmatterError::InvalidKey);
    }
    let split = split_fences(text)?;
    let before = match split.header {
        Some(header) => parse_mapping(header.yaml)?,
        None => Mapping::new(),
    };
    let key_value = Value::String(key.to_string());
    let new_value = value
        .map(serde_yaml::to_value)
        .transpose()
        .map_err(|error| FrontmatterError::Invalid(error.to_string()))?;
    if before.get(&key_value) == new_value.as_ref() {
        return Ok(None);
    }

    let nl = split.newline;
    let Some(header) = split.header else {
        // `before` is empty here, so `new_value` is `Some`: a delete returned above.
        let block = key_block(key, new_value.as_ref(), nl)?;
        return Ok(Some(format!(
            "{}---{nl}{block}---{nl}{}",
            split.bom, split.body
        )));
    };

    let lines: Vec<&str> = header.yaml.split_inclusive('\n').collect();
    let mut yaml = String::with_capacity(header.yaml.len() + 64);
    let block = new_value
        .as_ref()
        .map(|value| key_block(key, Some(value), nl))
        .transpose()?;
    match find_key_block(&lines, &key_value) {
        Some(range) => {
            lines[..range.start]
                .iter()
                .for_each(|line| yaml.push_str(line));
            if let Some(block) = &block {
                yaml.push_str(block);
            }
            lines[range.end..]
                .iter()
                .for_each(|line| yaml.push_str(line));
        }
        None => {
            yaml.push_str(header.yaml);
            if let Some(block) = &block {
                yaml.push_str(block);
            }
        }
    }

    verify_splice(&before, &yaml, &key_value, new_value.as_ref())?;

    let opening = &header.raw[..header.raw.find('\n').map_or(header.raw.len(), |at| at + 1)];
    let closing = &header.raw[opening.len() + header.yaml.len()..];
    if yaml.trim().is_empty() {
        // No keys and no comments remain: an empty fence pair has no purpose.
        return Ok(Some(format!("{}{}", split.bom, split.body)));
    }
    Ok(Some(format!(
        "{}{opening}{yaml}{closing}{}",
        split.bom, split.body
    )))
}

/// The lines that one top-level key occupies: `key: value` and the lines that
/// continue it. The block never ends with a blank line.
fn find_key_block(lines: &[&str], key: &Value) -> Option<std::ops::Range<usize>> {
    let start = lines
        .iter()
        .position(|line| top_level_key(content_of(line)).as_ref() == Some(key))?;
    let continues = |line: &&&str| {
        let line = content_of(line);
        line.trim().is_empty()
            || line.starts_with([' ', '\t'])
            || line == "-"
            || line.starts_with("- ")
    };
    let run = lines[start + 1..].iter().take_while(continues).count();
    let trailing_blanks = lines[start + 1..start + 1 + run]
        .iter()
        .rev()
        .take_while(|line| line.trim().is_empty())
        .count();
    Some(start..start + 1 + run - trailing_blanks)
}

/// The key of a column-0 `key:` line, parsed as YAML. `None` for any other line.
fn top_level_key(line: &str) -> Option<Value> {
    let token = if line.starts_with(['"', '\'']) {
        let quote = line.as_bytes()[0];
        let bytes = line.as_bytes();
        let mut at = 1;
        let end = loop {
            match bytes.get(at).copied()? {
                b'\\' if quote == b'"' => at += 2,
                b'\'' if quote == b'\'' && bytes.get(at + 1) == Some(&b'\'') => at += 2,
                byte if byte == quote => break at + 1,
                _ => at += 1,
            }
        };
        let after = line.get(end..)?.trim_start_matches([' ', '\t']);
        if !after.starts_with(':') {
            return None;
        }
        &line[..end]
    } else {
        if line.is_empty()
            || line.starts_with([' ', '\t', '#', '-', '?', '{', '[', '&', '*', '!', '|', '>'])
        {
            return None;
        }
        // A plain key ends at the first `:` that a space, a tab or the line end follows.
        let colon = line.match_indices(':').map(|(at, _)| at).find(|&at| {
            line[at + 1..]
                .chars()
                .next()
                .is_none_or(|next| next == ' ' || next == '\t')
        })?;
        line[..colon].trim_end()
    };
    serde_yaml::from_str(token).ok()
}

/// `key: value` lines for one key, with the note's line endings.
fn key_block(key: &str, value: Option<&Value>, nl: &str) -> Result<String, FrontmatterError> {
    let mut mapping = Mapping::new();
    let key_value = Value::String(key.to_string());
    mapping.insert(key_value, value.cloned().unwrap_or(Value::Null));
    let mut block = serde_yaml::to_string(&mapping)
        .map_err(|error| FrontmatterError::Invalid(error.to_string()))?;
    if matches!(value, None | Some(Value::Null)) {
        // Obsidian writes an empty property as `key:` with no value.
        if let Some(bare) = block.strip_suffix(" null\n") {
            block = format!("{bare}\n");
        }
    }
    Ok(if nl == "\n" {
        block
    } else {
        block.replace('\n', nl)
    })
}

/// Check that the spliced YAML holds the requested value for `key` and the
/// same values as `before` for all other keys.
fn verify_splice(
    before: &Mapping,
    yaml: &str,
    key: &Value,
    value: Option<&Value>,
) -> Result<(), FrontmatterError> {
    let mut after = parse_mapping(yaml)?;
    let mut expected = before.clone();
    if after.remove(key).as_ref() != value {
        return Err(FrontmatterError::Invalid(
            "the property did not take the requested value".to_string(),
        ));
    }
    expected.remove(key);
    if after != expected {
        return Err(FrontmatterError::Invalid(
            "the change would alter other properties".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(text: &str, key: &str, value: &serde_json::Value) -> String {
        set_frontmatter_key(text, key, Some(value))
            .expect("set succeeds")
            .expect("text changes")
    }

    fn delete(text: &str, key: &str) -> Option<String> {
        set_frontmatter_key(text, key, None).expect("delete succeeds")
    }

    #[test]
    fn split_is_byte_exact() {
        let text = "\u{feff}---\r\na: 1\r\n---\r\nbody\r\n";
        let split = split_yaml_frontmatter(text).unwrap();
        let header = split.header.unwrap();
        assert_eq!(split.newline, "\r\n");
        assert_eq!(split.bom, BOM);
        assert_eq!(header.yaml, "a: 1\r\n");
        assert_eq!(format!("{}{}{}", split.bom, header.raw, split.body), text);
        assert_eq!(split.body, "body\r\n");
    }

    #[test]
    fn split_accepts_dot_closing_fence_and_no_header() {
        let split = split_yaml_frontmatter("---\na: 1\n...\nbody").unwrap();
        assert_eq!(split.body, "body");
        let split = split_yaml_frontmatter("# Title\n---\n").unwrap();
        assert!(split.header.is_none());
        assert_eq!(split.body, "# Title\n---\n");
    }

    #[test]
    fn unclosed_header_is_an_error() {
        assert_eq!(
            split_yaml_frontmatter("---\na: 1\nbody\n"),
            Err(FrontmatterError::Unclosed)
        );
        assert_eq!(
            set_frontmatter_key("---\na: 1\n", "b", Some(&json!(2))),
            Err(FrontmatterError::Unclosed)
        );
    }

    #[test]
    fn toml_header_is_an_error() {
        assert_eq!(
            set_frontmatter_key("+++\na = 1\n+++\n", "b", Some(&json!(2))),
            Err(FrontmatterError::Toml)
        );
    }

    #[test]
    fn non_mapping_header_is_invalid() {
        assert!(matches!(
            split_yaml_frontmatter("---\n- a\n---\n"),
            Err(FrontmatterError::Invalid(_))
        ));
        assert!(matches!(
            frontmatter_mapping("---\na: [\n---\n"),
            Err(FrontmatterError::Invalid(_))
        ));
        assert_eq!(frontmatter_mapping("---\n---\n").unwrap(), Mapping::new());
        assert_eq!(frontmatter_mapping("body").unwrap(), Mapping::new());
    }

    #[test]
    fn comment_line_is_kept() {
        let text = "---\n# keep me\na: 1\n---\nbody";
        assert_eq!(set(text, "a", &json!(2)), "---\n# keep me\na: 2\n---\nbody");
    }

    #[test]
    fn flow_list_on_another_key_is_kept() {
        let text = "---\ntags: [a, b]\nstatus: open\n---\n";
        assert_eq!(
            set(text, "status", &json!("done")),
            "---\ntags: [a, b]\nstatus: done\n---\n"
        );
    }

    #[test]
    fn number_spelling_on_other_keys_is_kept() {
        let text = "---\nversion: 1.10\nid: 007\nx: 1\n---\n";
        assert_eq!(
            set(text, "x", &json!(2)),
            "---\nversion: 1.10\nid: 007\nx: 2\n---\n"
        );
    }

    #[test]
    fn anchor_and_alias_on_other_keys_are_kept() {
        let text = "---\na: &v hello\nb: *v\nc: 1\n---\n";
        assert_eq!(
            set(text, "c", &json!(2)),
            "---\na: &v hello\nb: *v\nc: 2\n---\n"
        );
    }

    #[test]
    fn replacing_the_anchor_that_an_alias_needs_is_invalid() {
        let text = "---\na: &v hello\nb: *v\n---\n";
        assert!(matches!(
            set_frontmatter_key(text, "a", Some(&json!("x"))),
            Err(FrontmatterError::Invalid(_))
        ));
    }

    #[test]
    fn key_that_the_line_scan_cannot_find_is_invalid_not_duplicated() {
        // `? k` is an explicit key: the mapping holds `k`, but no `k:` line exists.
        let text = "---\n? k\n: 1\n---\n";
        assert!(matches!(
            set_frontmatter_key(text, "k", Some(&json!(2))),
            Err(FrontmatterError::Invalid(_))
        ));
        assert!(matches!(
            set_frontmatter_key(text, "k", None),
            Err(FrontmatterError::Invalid(_))
        ));
    }

    #[test]
    fn block_sequence_value_is_replaced() {
        let text = "---\ntags:\n  - a\n  - b\nnext: 1\n---\n";
        assert_eq!(
            set(text, "tags", &json!("solo")),
            "---\ntags: solo\nnext: 1\n---\n"
        );
        let text = "---\ntags:\n- a\n- b\nnext: 1\n---\n";
        assert_eq!(
            set(text, "tags", &json!("solo")),
            "---\ntags: solo\nnext: 1\n---\n"
        );
    }

    #[test]
    fn nested_map_value_is_replaced() {
        let text = "---\nmeta:\n  author: me\n\n  year: 2020\n# after\nnext: 1\n---\n";
        assert_eq!(
            set(text, "meta", &json!(3)),
            "---\nmeta: 3\n# after\nnext: 1\n---\n"
        );
    }

    #[test]
    fn sequence_value_is_written_as_block() {
        let new = set("---\na: 1\n---\n", "tags", &json!(["x", "y"]));
        assert_eq!(
            frontmatter_mapping(&new).unwrap().get("tags"),
            Some(&serde_yaml::to_value(json!(["x", "y"])).unwrap())
        );
        assert!(new.starts_with("---\na: 1\ntags:\n"));
    }

    #[test]
    fn deleting_the_last_key_removes_the_header() {
        assert_eq!(delete("---\na: 1\n---\nbody\n", "a").unwrap(), "body\n");
        assert_eq!(
            delete("\u{feff}---\r\na: 1\r\n---\r\nbody", "a").unwrap(),
            "\u{feff}body"
        );
    }

    #[test]
    fn deleting_the_last_key_keeps_comments_and_fences() {
        assert_eq!(
            delete("---\n# note\na: 1\n---\nbody", "a").unwrap(),
            "---\n# note\n---\nbody"
        );
    }

    #[test]
    fn delete_keeps_other_keys() {
        assert_eq!(
            delete("---\na: 1\nb:\n  - x\nc: 3\n---\n", "b").unwrap(),
            "---\na: 1\nc: 3\n---\n"
        );
    }

    #[test]
    fn delete_without_frontmatter_or_key_is_none() {
        assert_eq!(delete("# body\n", "a"), None);
        assert_eq!(delete("---\nb: 1\n---\n", "a"), None);
    }

    #[test]
    fn bom_is_kept_and_no_second_header_is_made() {
        let text = "\u{feff}---\na: 1\n---\nbody";
        assert_eq!(
            set(text, "b", &json!(2)),
            "\u{feff}---\na: 1\nb: 2\n---\nbody"
        );
        assert_eq!(
            set("\u{feff}body", "b", &json!(2)),
            "\u{feff}---\nb: 2\n---\nbody"
        );
    }

    #[test]
    fn crlf_is_kept_on_every_line() {
        let text = "---\r\na: 1\r\n---\r\nbody\r\nmore\r\n";
        let new = set(text, "tags", &json!(["x"]));
        assert!(new.ends_with("---\r\nbody\r\nmore\r\n"));
        assert!(!new.replace("\r\n", "").contains('\n'), "{new:?}");
        let new = set("body\r\n", "a", &json!(1));
        assert_eq!(new, "---\r\na: 1\r\n---\r\nbody\r\n");
    }

    #[test]
    fn body_bytes_are_kept() {
        let body = "\n# Title\n---\nnot: frontmatter\n...\n  trailing  ";
        let text = format!("---\na: 1\n---{body}");
        let text = text.replacen("---# ", "---\n# ", 1);
        let new = set(&text, "a", &json!(2));
        assert_eq!(
            split_yaml_frontmatter(&new).unwrap().body,
            split_yaml_frontmatter(&text).unwrap().body
        );
    }

    #[test]
    fn equal_value_is_none() {
        let text = "---\nn: 1.5\ntags: [a, b]\nempty:\n---\n";
        assert_eq!(set_frontmatter_key(text, "n", Some(&json!(1.5))), Ok(None));
        assert_eq!(
            set_frontmatter_key(text, "tags", Some(&json!(["a", "b"]))),
            Ok(None)
        );
        assert_eq!(
            set_frontmatter_key(text, "empty", Some(&serde_json::Value::Null)),
            Ok(None)
        );
    }

    #[test]
    fn key_is_created_without_frontmatter() {
        assert_eq!(
            set("# Title\n", "status", &json!("open")),
            "---\nstatus: open\n---\n# Title\n"
        );
    }

    #[test]
    fn quoted_keys_are_found() {
        assert_eq!(
            set("---\n\"my key\": 1\n---\n", "my key", &json!(2)),
            "---\nmy key: 2\n---\n"
        );
        assert_eq!(
            set("---\n'it''s': 1\n---\n", "it's", &json!(2)),
            "---\nit's: 2\n---\n"
        );
    }

    #[test]
    fn json_null_writes_an_empty_property() {
        assert_eq!(
            set("---\na: 1\n---\n", "a", &serde_json::Value::Null),
            "---\na:\n---\n"
        );
    }

    #[test]
    fn key_with_line_break_is_rejected() {
        for key in ["", "a\nb", "a\rb"] {
            assert_eq!(
                set_frontmatter_key("---\na: 1\n---\n", key, Some(&json!(1))),
                Err(FrontmatterError::InvalidKey)
            );
        }
    }

    #[test]
    fn non_ascii_keys_are_found() {
        assert_eq!(
            set("---\ntítulo: a\nnext: 1\n---\n", "título", &json!("b")),
            "---\ntítulo: b\nnext: 1\n---\n"
        );
    }

    #[test]
    fn key_that_yaml_must_quote_is_quoted() {
        let new = set("---\na: 1\n---\n", "true", &json!(1));
        let map = frontmatter_mapping(&new).unwrap();
        assert_eq!(map.get("true"), Some(&Value::from(1)));
        assert_eq!(map.get("a"), Some(&Value::from(1)));
    }
}
