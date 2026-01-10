//! Utility functions shared across crates

/// Glob pattern matching using the `glob-match` crate
///
/// Supports:
/// - `*` matches any characters except path separators
/// - `**` matches any characters including path separators
/// - `?` matches a single character
/// - `[abc]` matches any character in the set
/// - `[a-z]` matches any character in the range
/// - `{a,b,c}` matches any of the alternatives
///
/// # Examples
///
/// ```
/// use crucible_core::utils::glob_match;
///
/// assert!(glob_match("just_*", "just_test"));
/// assert!(glob_match("tool:*", "tool:grep"));
/// assert!(glob_match("*.rs", "main.rs"));
/// assert!(glob_match("{foo,bar}_*", "foo_test"));
/// ```
#[inline]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match::glob_match(pattern, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("just_*", "just_test"));
        assert!(glob_match("just_*", "just_foo"));
        assert!(glob_match("*_test", "just_test"));
        assert!(!glob_match("just_*", "other_test"));
    }

    #[test]
    fn test_glob_match_question() {
        assert!(glob_match("ca?", "cat"));
        assert!(glob_match("ca?", "car"));
        assert!(!glob_match("ca?", "cart"));
    }

    #[test]
    fn test_glob_match_star_star() {
        assert!(glob_match("**/*.rs", "src/main.rs"));
        assert!(glob_match("**/*.rs", "src/lib/mod.rs"));
    }

    #[test]
    fn test_glob_match_braces() {
        assert!(glob_match("{foo,bar}_*", "foo_test"));
        assert!(glob_match("{foo,bar}_*", "bar_test"));
        assert!(!glob_match("{foo,bar}_*", "baz_test"));
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "other"));
    }

    #[test]
    fn test_glob_match_wildcard_all() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
    }
}
