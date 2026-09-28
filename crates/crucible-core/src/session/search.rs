//! The reply of `session.search`, and its text form for a person.
//!
//! `cru session search`, the TUI's `/search` and the web's `/search` all print
//! this reply, so the text form is here once.

use serde::{Deserialize, Serialize};

/// One transcript line that matched a session search.
///
/// Not a session: the daemon answers the line it matched on, so a caller that
/// wants the session reads `session_id` and asks for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionSearchMatch {
    pub session_id: String,
    /// The 1-based line of the transcript. `0` marks a title match on a
    /// session whose transcript has not reached disk yet.
    pub line: u64,
    /// The matched line, truncated to 100 characters.
    pub context: String,
}

/// What `session.search` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionSearchResponse {
    pub matches: Vec<SessionSearchMatch>,
    /// How many matches the reply carries.
    pub total: usize,
    /// Why the search looked at nothing, when it looked at nothing.
    ///
    /// The daemon writes it for a search with no kiln scope, and only then.
    /// An unscoped search is the one case where an empty result is not a
    /// statement about the corpus, so the sentence has to reach the caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl SessionSearchResponse {
    /// The reply as lines for a person: a header, then one line per match.
    pub fn to_text(&self, query: &str) -> String {
        if self.matches.is_empty() {
            return match &self.note {
                Some(note) => format!("No results found for '{query}': {note}"),
                None => format!("No results found for '{query}'"),
            };
        }
        let mut lines = vec![format!(
            "Search results for '{query}' ({} found):",
            self.total
        )];
        for (i, m) in self.matches.iter().enumerate() {
            // Line 0 has no line number worth printing.
            let location = if m.line == 0 {
                m.session_id.clone()
            } else {
                format!("{} (line {})", m.session_id, m.line)
            };
            lines.push(format!("  {}. {} — {}", i + 1, location, m.context));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_matches_read_as_a_header_and_two_lines() {
        let reply = SessionSearchResponse {
            matches: vec![
                SessionSearchMatch {
                    session_id: "s1".into(),
                    line: 3,
                    context: "one".into(),
                },
                SessionSearchMatch {
                    session_id: "s2".into(),
                    line: 0,
                    context: "two".into(),
                },
            ],
            total: 2,
            note: None,
        };
        assert_eq!(
            reply.to_text("q"),
            "Search results for 'q' (2 found):\n  1. s1 (line 3) — one\n  2. s2 — two"
        );
    }

    #[test]
    fn an_unscoped_search_says_why_it_found_nothing() {
        let reply = SessionSearchResponse {
            matches: Vec::new(),
            total: 0,
            note: Some("Specify 'kilns'".into()),
        };
        assert_eq!(
            reply.to_text("q"),
            "No results found for 'q': Specify 'kilns'"
        );
    }
}
