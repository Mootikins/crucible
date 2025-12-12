//! Regex module for Rune
//!
//! Provides regular expression support for Rune scripts by wrapping Rust's regex crate.
//!
//! # Example
//!
//! ```rune
//! use regex::{Regex, is_match, find, find_all, replace, replace_all, captures};
//!
//! // Simple matching
//! let pattern = Regex::new(r"\d+");
//! assert!(pattern.is_match("abc123"));
//!
//! // Find first match
//! if let Some(m) = pattern.find("abc123def456") {
//!     println!("Found: {}", m);  // "123"
//! }
//!
//! // Find all matches
//! let matches = pattern.find_all("abc123def456");
//! // matches == ["123", "456"]
//!
//! // Replace
//! let result = pattern.replace("abc123", "XXX");
//! // result == "abcXXX"
//!
//! // Captures
//! let pattern = Regex::new(r"(\w+)@(\w+)\.(\w+)");
//! if let Some(caps) = pattern.captures("user@example.com") {
//!     // caps[0] = "user@example.com" (full match)
//!     // caps[1] = "user"
//!     // caps[2] = "example"
//!     // caps[3] = "com"
//! }
//! ```

use regex::Regex as RustRegex;
use rune::alloc::Vec as RuneVec;
use rune::runtime::VmResult;
use rune::{Any, ContextError, Module};

/// Create the regex module for Rune
pub fn regex_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("regex")?;

    // Register the Regex type
    module.ty::<RuneRegex>()?;
    module.function_meta(RuneRegex::new)?;
    module.function_meta(RuneRegex::pattern)?;
    module.function_meta(RuneRegex::is_match)?;
    module.function_meta(RuneRegex::find)?;
    module.function_meta(RuneRegex::find_all)?;
    module.function_meta(RuneRegex::replace)?;
    module.function_meta(RuneRegex::replace_all)?;
    module.function_meta(RuneRegex::split)?;
    module.function_meta(RuneRegex::captures)?;

    // Convenience functions (don't require creating Regex object)
    module.function_meta(is_match)?;
    module.function_meta(find)?;
    module.function_meta(find_all)?;
    module.function_meta(replace)?;
    module.function_meta(replace_all)?;

    Ok(module)
}

/// Compiled regular expression for Rune
///
/// Available in Rune as `regex::Regex`
#[derive(Debug, Clone, Any)]
#[rune(item = ::regex, name = Regex)]
pub struct RuneRegex {
    inner: RustRegex,
    pattern: String,
}

impl RuneRegex {
    /// Create a new regex from a pattern
    ///
    /// # Errors
    /// Returns an error if the pattern is invalid.
    #[rune::function(path = Self::new)]
    pub fn new(pattern: &str) -> VmResult<Self> {
        match RustRegex::new(pattern) {
            Ok(inner) => VmResult::Ok(Self {
                inner,
                pattern: pattern.to_string(),
            }),
            Err(e) => VmResult::panic(format!("Invalid regex pattern '{}': {}", pattern, e)),
        }
    }

    /// Get the original pattern string
    #[rune::function(path = Self::pattern)]
    pub fn pattern(&self) -> String {
        self.pattern.clone()
    }

    /// Check if the pattern matches anywhere in the text
    #[rune::function(path = Self::is_match)]
    pub fn is_match(&self, text: String) -> bool {
        self.inner.is_match(&text)
    }

    /// Find the first match in the text
    ///
    /// Returns `Some(matched_text)` or `None` if no match.
    #[rune::function(path = Self::find)]
    pub fn find(&self, text: String) -> Option<String> {
        self.inner.find(&text).map(|m| m.as_str().to_string())
    }

    /// Find all matches in the text
    ///
    /// Returns a vector of all matched strings.
    #[rune::function(path = Self::find_all)]
    pub fn find_all(&self, text: String) -> VmResult<RuneVec<String>> {
        let matches: Vec<String> = self
            .inner
            .find_iter(&text)
            .map(|m| m.as_str().to_string())
            .collect();
        VmResult::Ok(RuneVec::try_from(matches).unwrap())
    }

    /// Replace the first match with the replacement string
    ///
    /// The replacement can use `$1`, `$2`, etc. for capture groups.
    #[rune::function(path = Self::replace)]
    pub fn replace(&self, text: String, replacement: String) -> String {
        self.inner.replace(&text, replacement.as_str()).into_owned()
    }

    /// Replace all matches with the replacement string
    ///
    /// The replacement can use `$1`, `$2`, etc. for capture groups.
    #[rune::function(path = Self::replace_all)]
    pub fn replace_all(&self, text: String, replacement: String) -> String {
        self.inner
            .replace_all(&text, replacement.as_str())
            .into_owned()
    }

    /// Split the text by the pattern
    ///
    /// Returns a vector of substrings.
    #[rune::function(path = Self::split)]
    pub fn split(&self, text: String) -> VmResult<RuneVec<String>> {
        let parts: Vec<String> = self.inner.split(&text).map(|s| s.to_string()).collect();
        VmResult::Ok(RuneVec::try_from(parts).unwrap())
    }

    /// Get capture groups from the first match
    ///
    /// Returns a vector where index 0 is the full match,
    /// and subsequent indices are the capture groups.
    /// Returns `None` if no match.
    #[rune::function(path = Self::captures)]
    pub fn captures(&self, text: String) -> VmResult<Option<RuneVec<Option<String>>>> {
        match self.inner.captures(&text) {
            Some(caps) => {
                let groups: Vec<Option<String>> = caps
                    .iter()
                    .map(|m| m.map(|m| m.as_str().to_string()))
                    .collect();
                VmResult::Ok(Some(RuneVec::try_from(groups).unwrap()))
            }
            None => VmResult::Ok(None),
        }
    }
}

// === Convenience functions ===

/// Check if a pattern matches anywhere in the text
///
/// This is a convenience function that compiles the regex each time.
/// For repeated use, create a `Regex` object instead.
#[rune::function]
fn is_match(pattern: String, text: String) -> VmResult<bool> {
    match RustRegex::new(&pattern) {
        Ok(re) => VmResult::Ok(re.is_match(&text)),
        Err(e) => VmResult::panic(format!("Invalid regex pattern '{}': {}", pattern, e)),
    }
}

/// Find the first match of a pattern in the text
#[rune::function]
fn find(pattern: String, text: String) -> VmResult<Option<String>> {
    match RustRegex::new(&pattern) {
        Ok(re) => VmResult::Ok(re.find(&text).map(|m| m.as_str().to_string())),
        Err(e) => VmResult::panic(format!("Invalid regex pattern '{}': {}", pattern, e)),
    }
}

/// Find all matches of a pattern in the text
#[rune::function]
fn find_all(pattern: String, text: String) -> VmResult<RuneVec<String>> {
    match RustRegex::new(&pattern) {
        Ok(re) => {
            let matches: Vec<String> = re
                .find_iter(&text)
                .map(|m| m.as_str().to_string())
                .collect();
            VmResult::Ok(RuneVec::try_from(matches).unwrap())
        }
        Err(e) => VmResult::panic(format!("Invalid regex pattern '{}': {}", pattern, e)),
    }
}

/// Replace the first match of a pattern
#[rune::function]
fn replace(pattern: String, text: String, replacement: String) -> VmResult<String> {
    match RustRegex::new(&pattern) {
        Ok(re) => VmResult::Ok(re.replace(&text, replacement.as_str()).into_owned()),
        Err(e) => VmResult::panic(format!("Invalid regex pattern '{}': {}", pattern, e)),
    }
}

/// Replace all matches of a pattern
#[rune::function]
fn replace_all(pattern: String, text: String, replacement: String) -> VmResult<String> {
    match RustRegex::new(&pattern) {
        Ok(re) => VmResult::Ok(re.replace_all(&text, replacement.as_str()).into_owned()),
        Err(e) => VmResult::panic(format!("Invalid regex pattern '{}': {}", pattern, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_module_creation() {
        let module = regex_module();
        assert!(module.is_ok(), "Should create regex module");
    }

    #[test]
    fn test_is_match_impl() {
        let re = RustRegex::new(r"\d+").unwrap();
        assert!(re.is_match("abc123"));
        assert!(!re.is_match("abcdef"));
    }

    #[test]
    fn test_find_impl() {
        let re = RustRegex::new(r"\d+").unwrap();
        let found = re.find("abc123def");
        assert_eq!(found.map(|m| m.as_str()), Some("123"));
    }

    #[test]
    fn test_find_all_impl() {
        let re = RustRegex::new(r"\d+").unwrap();
        let matches: Vec<&str> = re.find_iter("abc123def456").map(|m| m.as_str()).collect();
        assert_eq!(matches, vec!["123", "456"]);
    }

    #[test]
    fn test_replace_impl() {
        let re = RustRegex::new(r"\d+").unwrap();
        let result = re.replace("abc123def", "XXX");
        assert_eq!(result, "abcXXXdef");
    }

    #[test]
    fn test_replace_all_impl() {
        let re = RustRegex::new(r"\d+").unwrap();
        let result = re.replace_all("abc123def456", "XXX");
        assert_eq!(result, "abcXXXdefXXX");
    }

    #[test]
    fn test_split_impl() {
        let re = RustRegex::new(r"[,;]").unwrap();
        let parts: Vec<&str> = re.split("a,b;c,d").collect();
        assert_eq!(parts, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_captures_impl() {
        let re = RustRegex::new(r"(\w+)@(\w+)\.(\w+)").unwrap();
        let caps = re.captures("user@example.com").unwrap();
        assert_eq!(caps.get(0).map(|m| m.as_str()), Some("user@example.com"));
        assert_eq!(caps.get(1).map(|m| m.as_str()), Some("user"));
        assert_eq!(caps.get(2).map(|m| m.as_str()), Some("example"));
        assert_eq!(caps.get(3).map(|m| m.as_str()), Some("com"));
    }
}
