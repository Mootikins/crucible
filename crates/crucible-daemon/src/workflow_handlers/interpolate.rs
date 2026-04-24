//! Step-body output interpolation.
//!
//! Resolves `**name**` markdown-bold tokens against an
//! [`OutputScope`][crucible_core::workflow::OutputScope]. A token is
//! interpolated only when its name exactly matches a scope key —
//! genuine prose emphasis passes through unchanged. JSON values are
//! inlined as their text for strings, pretty-printed otherwise.

use crucible_core::workflow::OutputScope;
use regex::{Captures, Regex};
use std::sync::OnceLock;

/// Replace `**name**` tokens whose name matches a scope key with the
/// scope value's text form. Unknown names are left as literal
/// `**name**` markdown.
pub fn interpolate(body: &str, scope: &OutputScope) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\*\*([A-Za-z_][A-Za-z0-9_-]*)\*\*").unwrap());

    re.replace_all(body, |caps: &Captures| {
        let name = &caps[1];
        match scope.get(name) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => {
                serde_json::to_string_pretty(other).unwrap_or_else(|_| caps[0].to_string())
            }
            None => caps[0].to_string(),
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope_of(entries: &[(&str, serde_json::Value)]) -> OutputScope {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn string_value_inlines_verbatim() {
        let scope = scope_of(&[("name", json!("Alice"))]);
        assert_eq!(interpolate("Hello, **name**!", &scope), "Hello, Alice!");
    }

    #[test]
    fn json_value_pretty_prints() {
        let scope = scope_of(&[("config", json!({"port": 8080}))]);
        let out = interpolate("Use **config** here.", &scope);
        assert!(out.contains("\"port\": 8080"));
    }

    #[test]
    fn unmatched_token_stays_literal() {
        let scope = OutputScope::new();
        assert_eq!(interpolate("**bold prose**", &scope), "**bold prose**");
    }

    #[test]
    fn hyphenated_name_resolves() {
        let scope = scope_of(&[("plan-a", json!("ship it"))]);
        assert_eq!(interpolate("do **plan-a**", &scope), "do ship it");
    }

    #[test]
    fn mixed_matched_and_unmatched() {
        let scope = scope_of(&[("x", json!("1"))]);
        assert_eq!(
            interpolate("**emphasis** plus **x**", &scope),
            "**emphasis** plus 1"
        );
    }

    #[test]
    fn non_identifier_tokens_ignored() {
        // Spaces, punctuation inside `**...**` — not an identifier, not an
        // interpolation candidate.
        let scope = scope_of(&[("x", json!("v"))]);
        assert_eq!(
            interpolate("**not a name** **x**", &scope),
            "**not a name** v"
        );
    }
}
