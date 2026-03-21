//! Auto-linking: detect unlinked note mentions in text.
//!
//! Scans text for note names that appear as plain text but aren't already
//! wrapped in `[[wikilink]]` syntax, returning suggestions for the caller
//! to present or apply.

use serde::Serialize;
use std::collections::HashSet;

/// A single suggestion to convert a plain-text mention into a wikilink.
#[derive(Debug, Serialize)]
pub struct LinkSuggestion {
    /// The text that was found as a mention (preserves original casing)
    pub mention: String,
    /// The note name to link to
    pub target: String,
    /// Byte offset in the text where the mention starts
    pub offset: usize,
}

/// Find note names mentioned in text that aren't already wikilinked.
///
/// Returns at most one suggestion per note name, preferring the first
/// word-boundary-aligned occurrence. Names shorter than 3 characters are
/// skipped to avoid false positives on common short words.
///
/// Note: Currently ASCII-safe only. If the input text or note names contain
/// characters that change byte length when lowercased (rare in practice),
/// the function returns empty/partial results rather than incorrect offsets.
pub fn suggest_links(text: &str, note_names: &[String]) -> Vec<LinkSuggestion> {
    let existing_links: HashSet<String> = extract_wikilink_targets(text)
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();

    let wikilink_spans = collect_wikilink_spans(text);

    let text_lower = text.to_lowercase();
    let mut suggestions = Vec::new();

    // If the text contains characters that change byte length when lowercased,
    // all byte offsets from find() on the lowercased text would be wrong.
    // Fall back to empty results in this case.
    if text.len() != text_lower.len() {
        return suggestions;
    }

    for name in note_names {
        if name.len() < 3 {
            continue;
        }

        if existing_links.contains(&name.to_lowercase()) {
            continue;
        }

        let name_lower = name.to_lowercase();

        // Guard: if lowercasing changed byte length, skip this name to avoid
        // incorrect byte offsets. Full Unicode support is a future enhancement.
        if name.len() != name_lower.len() {
            continue;
        }

        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(&name_lower) {
            let abs_pos = search_from + pos;
            let end_pos = abs_pos + name_lower.len();

            // Word boundary: character before must not be alphanumeric/underscore
            let start_ok = abs_pos == 0
                || !text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                    && text.as_bytes()[abs_pos - 1] != b'_';
            // Word boundary: character after must not be alphanumeric/underscore
            let end_ok = end_pos >= text.len()
                || !text.as_bytes()[end_pos].is_ascii_alphanumeric()
                    && text.as_bytes()[end_pos] != b'_';

            let in_wikilink = wikilink_spans
                .iter()
                .any(|&(start, end)| abs_pos >= start && abs_pos < end);

            if start_ok && end_ok && !in_wikilink {
                suggestions.push(LinkSuggestion {
                    mention: text[abs_pos..end_pos].to_string(),
                    target: name.clone(),
                    offset: abs_pos,
                });
                break; // one suggestion per note name
            }

            search_from = abs_pos + 1;
        }
    }

    suggestions
}

/// Extract wikilink targets from text (simple `[[target]]` extraction).
fn extract_wikilink_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i + 2;
            if let Some(end) = text[start..].find("]]") {
                let content = &text[start..start + end];
                // Handle aliases: [[target|alias]]
                let target = content.split('|').next().unwrap_or(content);
                // Handle heading refs: [[target#heading]]
                let target = target.split('#').next().unwrap_or(target);
                targets.push(target.trim().to_string());
                i = start + end + 2;
            } else {
                i += 2;
            }
        } else {
            i += 1;
        }
    }

    targets
}

/// Collect byte spans `(start, end)` of all `[[...]]` sequences so we can
/// avoid suggesting matches that fall inside an existing wikilink.
fn collect_wikilink_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i;
            if let Some(end_offset) = text[i + 2..].find("]]") {
                let end = i + 2 + end_offset + 2; // include closing ]]
                spans.push((start, end));
                i = end;
            } else {
                i += 2;
            }
        } else {
            i += 1;
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_unlinked_mentions() {
        let text = "I've been learning Rust and TypeScript lately.";
        let notes = vec![
            "Rust".to_string(),
            "TypeScript".to_string(),
            "Go".to_string(),
        ];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].target, "Rust");
        assert_eq!(suggestions[0].mention, "Rust");
        assert_eq!(suggestions[1].target, "TypeScript");
    }

    #[test]
    fn skips_already_linked() {
        let text = "I use [[Rust]] and TypeScript daily.";
        let notes = vec!["Rust".to_string(), "TypeScript".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].target, "TypeScript");
    }

    #[test]
    fn skips_short_names() {
        let text = "I use Go and Rust for backend.";
        let notes = vec!["Go".to_string(), "Rust".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].target, "Rust");
    }

    #[test]
    fn respects_word_boundaries() {
        let text = "The rusty car needs a rust treatment.";
        let notes = vec!["Rust".to_string()];
        let suggestions = suggest_links(text, &notes);
        // "rusty" is not a match (no word boundary after), but "rust" is
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].mention, "rust");
        assert_eq!(suggestions[0].offset, 22);
    }

    #[test]
    fn case_insensitive_matching() {
        let text = "I wrote about rust today.";
        let notes = vec!["Rust".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].mention, "rust");
        assert_eq!(suggestions[0].target, "Rust");
    }

    #[test]
    fn handles_alias_wikilinks() {
        let text = "See [[Rust|the Rust language]] and also TypeScript.";
        let notes = vec!["Rust".to_string(), "TypeScript".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].target, "TypeScript");
    }

    #[test]
    fn handles_heading_wikilinks() {
        let text = "See [[Rust#Ownership]] for details on TypeScript.";
        let notes = vec!["Rust".to_string(), "TypeScript".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].target, "TypeScript");
    }

    #[test]
    fn no_suggestions_when_all_linked() {
        let text = "I use [[Rust]] and [[TypeScript]] daily.";
        let notes = vec!["Rust".to_string(), "TypeScript".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn no_suggestions_when_no_matches() {
        let text = "I enjoy reading books.";
        let notes = vec!["Rust".to_string(), "TypeScript".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn does_not_match_inside_wikilink_text() {
        // "Rust" appears inside the wikilink alias, should not be suggested
        let text = "See [[Programming|Learn Rust here]] for more.";
        let notes = vec!["Rust".to_string(), "Programming".to_string()];
        let suggestions = suggest_links(text, &notes);
        // "Programming" is already linked, "Rust" is inside a wikilink span
        assert!(suggestions.is_empty());
    }

    #[test]
    fn returns_correct_offsets() {
        let text = "Hello Rust world";
        let notes = vec!["Rust".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].offset, 6);
        assert_eq!(&text[6..10], "Rust");
    }

    #[test]
    fn handles_empty_inputs() {
        assert!(suggest_links("", &[]).is_empty());
        assert!(suggest_links("some text", &[]).is_empty());
        assert!(suggest_links("", &["Rust".to_string()]).is_empty());
    }

    #[test]
    fn multi_word_note_names() {
        let text = "I read about Design Patterns and clean code.";
        let notes = vec!["Design Patterns".to_string(), "Clean Code".to_string()];
        let suggestions = suggest_links(text, &notes);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].target, "Design Patterns");
        assert_eq!(suggestions[1].target, "Clean Code");
        assert_eq!(suggestions[1].mention, "clean code");
    }

    #[test]
    fn underscore_is_not_word_boundary() {
        let text = "The rust_lang crate is great. Also Rust is cool.";
        let notes = vec!["Rust".to_string()];
        let suggestions = suggest_links(text, &notes);
        // "rust_lang" should NOT match, but standalone "Rust" should
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].mention, "Rust");
        assert_eq!(suggestions[0].offset, 35);
    }
}
