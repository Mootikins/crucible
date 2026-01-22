//! Configuration value types for vim-style `:set` command system.
//!
//! This module provides [`ConfigValue`], a dynamic value type that supports
//! type coercion and parsing for configuration options.
//!
//! # Examples
//!
//! ```
//! use crucible_cli::tui::oil::config::ConfigValue;
//!
//! // Parse with auto-detection
//! let val = ConfigValue::parse("true", None);
//! assert_eq!(val.as_bool(), Some(true));
//!
//! // Parse with type hint
//! let hint = ConfigValue::Int(0);
//! let val = ConfigValue::parse("42", Some(&hint));
//! assert_eq!(val.as_int(), Some(42));
//! ```

use std::fmt;

/// A configuration value that can hold different types.
///
/// Used in vim-style `:set` commands to represent option values
/// with automatic type coercion and parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    /// A string value.
    String(String),
    /// An integer value (64-bit signed).
    Int(i64),
    /// A floating-point value (64-bit).
    Float(f64),
    /// A boolean value.
    Bool(bool),
    /// A complex JSON value for nested structures.
    Json(serde_json::Value),
}

impl ConfigValue {
    /// Parse a string into a `ConfigValue`, optionally using a type hint.
    ///
    /// If a hint is provided, attempts to parse as that type first.
    /// Otherwise, auto-detects the type in order: bool, int, float, string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse
    /// * `hint` - Optional type hint to guide parsing
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// // Auto-detect boolean
    /// let val = ConfigValue::parse("yes", None);
    /// assert_eq!(val, ConfigValue::Bool(true));
    ///
    /// // Auto-detect integer
    /// let val = ConfigValue::parse("42", None);
    /// assert_eq!(val, ConfigValue::Int(42));
    ///
    /// // Use type hint to parse as float
    /// let hint = ConfigValue::Float(0.0);
    /// let val = ConfigValue::parse("42", Some(&hint));
    /// assert_eq!(val, ConfigValue::Float(42.0));
    /// ```
    pub fn parse(s: &str, hint: Option<&ConfigValue>) -> Self {
        // If we have a type hint, try to parse as that type first
        if let Some(hint) = hint {
            if let Some(val) = Self::try_parse_as_type(s, hint) {
                return val;
            }
        }

        // Auto-detect type: bool -> int -> float -> string
        if let Ok(b) = Self::try_parse_bool(s) {
            return ConfigValue::Bool(b);
        }

        if let Ok(i) = s.parse::<i64>() {
            return ConfigValue::Int(i);
        }

        if let Ok(f) = s.parse::<f64>() {
            return ConfigValue::Float(f);
        }

        ConfigValue::String(s.to_string())
    }

    /// Try to parse `s` as the same type as `hint`.
    fn try_parse_as_type(s: &str, hint: &ConfigValue) -> Option<Self> {
        match hint {
            ConfigValue::Bool(_) => Self::try_parse_bool(s).ok().map(ConfigValue::Bool),
            ConfigValue::Int(_) => s.parse::<i64>().ok().map(ConfigValue::Int),
            ConfigValue::Float(_) => s.parse::<f64>().ok().map(ConfigValue::Float),
            ConfigValue::String(_) => Some(ConfigValue::String(s.to_string())),
            ConfigValue::Json(_) => serde_json::from_str(s).ok().map(ConfigValue::Json),
        }
    }

    /// Parse a string to a boolean value.
    ///
    /// Accepts: `true`, `false`, `1`, `0`, `yes`, `no`, `on`, `off`, `y`, `n`
    /// (case-insensitive).
    ///
    /// # Panics
    ///
    /// Panics if the string is not a recognized boolean value.
    /// Use [`try_parse_bool`](Self::try_parse_bool) for a non-panicking version.
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// assert!(ConfigValue::parse_bool("yes"));
    /// assert!(ConfigValue::parse_bool("TRUE"));
    /// assert!(ConfigValue::parse_bool("1"));
    /// assert!(!ConfigValue::parse_bool("no"));
    /// assert!(!ConfigValue::parse_bool("OFF"));
    /// ```
    #[must_use]
    pub fn parse_bool(s: &str) -> bool {
        Self::try_parse_bool(s).expect("invalid boolean value")
    }

    /// Try to parse a string as a boolean value.
    ///
    /// Accepts: `true`, `false`, `1`, `0`, `yes`, `no`, `on`, `off`, `y`, `n`
    /// (case-insensitive).
    ///
    /// # Returns
    ///
    /// - `Ok(true)` for truthy values
    /// - `Ok(false)` for falsy values
    /// - `Err(())` if the string is not a recognized boolean
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// assert_eq!(ConfigValue::try_parse_bool("yes"), Ok(true));
    /// assert_eq!(ConfigValue::try_parse_bool("no"), Ok(false));
    /// assert!(ConfigValue::try_parse_bool("maybe").is_err());
    /// ```
    pub fn try_parse_bool(s: &str) -> Result<bool, ()> {
        match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" | "y" => Ok(true),
            "false" | "0" | "no" | "off" | "n" => Ok(false),
            _ => Err(()),
        }
    }

    /// Coerce the value to a boolean.
    ///
    /// - `Bool`: returns the value directly
    /// - `Int`: returns `true` if non-zero
    /// - `Float`: returns `true` if non-zero
    /// - `String`: attempts to parse as boolean
    /// - `Json`: returns `None` (complex types don't coerce to bool)
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// assert_eq!(ConfigValue::Bool(true).as_bool(), Some(true));
    /// assert_eq!(ConfigValue::Int(1).as_bool(), Some(true));
    /// assert_eq!(ConfigValue::Int(0).as_bool(), Some(false));
    /// assert_eq!(ConfigValue::String("yes".into()).as_bool(), Some(true));
    /// ```
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Bool(b) => Some(*b),
            ConfigValue::Int(i) => Some(*i != 0),
            ConfigValue::Float(f) => Some(*f != 0.0),
            ConfigValue::String(s) => Self::try_parse_bool(s).ok(),
            ConfigValue::Json(_) => None,
        }
    }

    /// Get the value as a string reference.
    ///
    /// Only returns `Some` for `String` variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// let val = ConfigValue::String("hello".into());
    /// assert_eq!(val.as_string(), Some("hello"));
    ///
    /// let val = ConfigValue::Int(42);
    /// assert_eq!(val.as_string(), None);
    /// ```
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get the value as an integer.
    ///
    /// - `Int`: returns the value directly
    /// - `Float`: returns the truncated integer if it fits in i64
    /// - `Bool`: returns 1 for true, 0 for false
    /// - `String`: attempts to parse as integer
    /// - `Json`: returns `None`
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// assert_eq!(ConfigValue::Int(42).as_int(), Some(42));
    /// assert_eq!(ConfigValue::Float(3.7).as_int(), Some(3));
    /// assert_eq!(ConfigValue::Bool(true).as_int(), Some(1));
    /// assert_eq!(ConfigValue::String("123".into()).as_int(), Some(123));
    /// ```
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConfigValue::Int(i) => Some(*i),
            ConfigValue::Float(f) => {
                // Check if the float can be safely converted to i64
                if f.is_finite() && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 {
                    Some(*f as i64)
                } else {
                    None
                }
            }
            ConfigValue::Bool(b) => Some(if *b { 1 } else { 0 }),
            ConfigValue::String(s) => s.parse().ok(),
            ConfigValue::Json(_) => None,
        }
    }

    /// Get the value as a float.
    ///
    /// - `Float`: returns the value directly
    /// - `Int`: converts to float
    /// - `Bool`: returns 1.0 for true, 0.0 for false
    /// - `String`: attempts to parse as float
    /// - `Json`: returns `None`
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// assert_eq!(ConfigValue::Float(3.14).as_float(), Some(3.14));
    /// assert_eq!(ConfigValue::Int(42).as_float(), Some(42.0));
    /// assert_eq!(ConfigValue::Bool(true).as_float(), Some(1.0));
    /// assert_eq!(ConfigValue::String("2.5".into()).as_float(), Some(2.5));
    /// ```
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(f) => Some(*f),
            ConfigValue::Int(i) => Some(*i as f64),
            ConfigValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            ConfigValue::String(s) => s.parse().ok(),
            ConfigValue::Json(_) => None,
        }
    }

    /// Get the JSON value if this is a `Json` variant.
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    /// use serde_json::json;
    ///
    /// let val = ConfigValue::Json(json!({"key": "value"}));
    /// assert!(val.as_json().is_some());
    ///
    /// let val = ConfigValue::Int(42);
    /// assert!(val.as_json().is_none());
    /// ```
    #[must_use]
    pub fn as_json(&self) -> Option<&serde_json::Value> {
        match self {
            ConfigValue::Json(v) => Some(v),
            _ => None,
        }
    }

    /// Get the type name of this value.
    ///
    /// Returns one of: `"string"`, `"integer"`, `"float"`, `"boolean"`, `"json"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use crucible_cli::tui::oil::config::ConfigValue;
    ///
    /// assert_eq!(ConfigValue::String("hello".into()).type_name(), "string");
    /// assert_eq!(ConfigValue::Int(42).type_name(), "integer");
    /// assert_eq!(ConfigValue::Float(3.14).type_name(), "float");
    /// assert_eq!(ConfigValue::Bool(true).type_name(), "boolean");
    /// ```
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            ConfigValue::String(_) => "string",
            ConfigValue::Int(_) => "integer",
            ConfigValue::Float(_) => "float",
            ConfigValue::Bool(_) => "boolean",
            ConfigValue::Json(_) => "json",
        }
    }
}

impl fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigValue::String(s) => write!(f, "{s}"),
            ConfigValue::Int(i) => write!(f, "{i}"),
            ConfigValue::Float(fl) => write!(f, "{fl}"),
            ConfigValue::Bool(b) => write!(f, "{b}"),
            ConfigValue::Json(v) => write!(f, "{v}"),
        }
    }
}

impl From<serde_json::Value> for ConfigValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => ConfigValue::String(String::new()),
            serde_json::Value::Bool(b) => ConfigValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ConfigValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    ConfigValue::Float(f)
                } else {
                    // Fallback for very large numbers
                    ConfigValue::String(n.to_string())
                }
            }
            serde_json::Value::String(s) => ConfigValue::String(s),
            // Arrays and objects become Json
            v @ (serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
                ConfigValue::Json(v)
            }
        }
    }
}

impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        ConfigValue::String(s)
    }
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        ConfigValue::String(s.to_string())
    }
}

impl From<i64> for ConfigValue {
    fn from(i: i64) -> Self {
        ConfigValue::Int(i)
    }
}

impl From<i32> for ConfigValue {
    fn from(i: i32) -> Self {
        ConfigValue::Int(i64::from(i))
    }
}

impl From<f64> for ConfigValue {
    fn from(f: f64) -> Self {
        ConfigValue::Float(f)
    }
}

impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self {
        ConfigValue::Bool(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==================== Bool Parsing Tests ====================

    #[test]
    fn parse_bool_truthy_values() {
        for input in &[
            "true", "TRUE", "True", "1", "yes", "YES", "on", "ON", "y", "Y",
        ] {
            assert!(
                ConfigValue::try_parse_bool(input).unwrap(),
                "Expected truthy for: {input}"
            );
        }
    }

    #[test]
    fn parse_bool_falsy_values() {
        for input in &[
            "false", "FALSE", "False", "0", "no", "NO", "off", "OFF", "n", "N",
        ] {
            assert!(
                !ConfigValue::try_parse_bool(input).unwrap(),
                "Expected falsy for: {input}"
            );
        }
    }

    #[test]
    fn parse_bool_invalid_values() {
        for input in &["maybe", "2", "yep", "nope", "", "truee", "fals"] {
            assert!(
                ConfigValue::try_parse_bool(input).is_err(),
                "Expected error for: {input}"
            );
        }
    }

    #[test]
    #[should_panic(expected = "invalid boolean value")]
    fn parse_bool_panics_on_invalid() {
        let _ = ConfigValue::parse_bool("invalid");
    }

    // ==================== Auto-Detection Tests ====================

    #[test]
    fn parse_auto_detects_bool() {
        assert_eq!(ConfigValue::parse("true", None), ConfigValue::Bool(true));
        assert_eq!(ConfigValue::parse("false", None), ConfigValue::Bool(false));
        assert_eq!(ConfigValue::parse("yes", None), ConfigValue::Bool(true));
        assert_eq!(ConfigValue::parse("no", None), ConfigValue::Bool(false));
    }

    #[test]
    fn parse_auto_detects_int() {
        assert_eq!(ConfigValue::parse("42", None), ConfigValue::Int(42));
        assert_eq!(ConfigValue::parse("-100", None), ConfigValue::Int(-100));
        assert_eq!(
            ConfigValue::parse("9223372036854775807", None),
            ConfigValue::Int(i64::MAX)
        );
    }

    #[test]
    fn parse_auto_detects_float() {
        assert_eq!(ConfigValue::parse("3.14", None), ConfigValue::Float(3.14));
        assert_eq!(ConfigValue::parse("-2.5", None), ConfigValue::Float(-2.5));
        assert_eq!(ConfigValue::parse("1e10", None), ConfigValue::Float(1e10));
    }

    #[test]
    fn parse_falls_back_to_string() {
        assert_eq!(
            ConfigValue::parse("hello", None),
            ConfigValue::String("hello".into())
        );
        assert_eq!(
            ConfigValue::parse("hello world", None),
            ConfigValue::String("hello world".into())
        );
        assert_eq!(
            ConfigValue::parse("", None),
            ConfigValue::String(String::new())
        );
    }

    // ==================== Type Hint Tests ====================

    #[test]
    fn parse_with_int_hint() {
        let hint = ConfigValue::Int(0);
        assert_eq!(ConfigValue::parse("42", Some(&hint)), ConfigValue::Int(42));
        // Invalid int falls back to auto-detect -> string
        assert_eq!(
            ConfigValue::parse("hello", Some(&hint)),
            ConfigValue::String("hello".into())
        );
    }

    #[test]
    fn parse_with_float_hint() {
        let hint = ConfigValue::Float(0.0);
        // Int string parses as float when hinted
        assert_eq!(
            ConfigValue::parse("42", Some(&hint)),
            ConfigValue::Float(42.0)
        );
        assert_eq!(
            ConfigValue::parse("3.14", Some(&hint)),
            ConfigValue::Float(3.14)
        );
    }

    #[test]
    fn parse_with_bool_hint() {
        let hint = ConfigValue::Bool(false);
        assert_eq!(
            ConfigValue::parse("yes", Some(&hint)),
            ConfigValue::Bool(true)
        );
        assert_eq!(
            ConfigValue::parse("0", Some(&hint)),
            ConfigValue::Bool(false)
        );
        // "42" is not a valid bool, falls back to int auto-detect
        assert_eq!(ConfigValue::parse("42", Some(&hint)), ConfigValue::Int(42));
    }

    #[test]
    fn parse_with_string_hint() {
        let hint = ConfigValue::String(String::new());
        // Everything becomes a string with string hint
        assert_eq!(
            ConfigValue::parse("42", Some(&hint)),
            ConfigValue::String("42".into())
        );
        assert_eq!(
            ConfigValue::parse("true", Some(&hint)),
            ConfigValue::String("true".into())
        );
    }

    #[test]
    fn parse_with_json_hint() {
        let hint = ConfigValue::Json(json!({}));
        let parsed = ConfigValue::parse(r#"{"key": "value"}"#, Some(&hint));
        assert_eq!(parsed, ConfigValue::Json(json!({"key": "value"})));

        // Invalid JSON falls back to auto-detect
        assert_eq!(
            ConfigValue::parse("not json", Some(&hint)),
            ConfigValue::String("not json".into())
        );
    }

    // ==================== Coercion Tests ====================

    #[test]
    fn as_bool_coercion() {
        assert_eq!(ConfigValue::Bool(true).as_bool(), Some(true));
        assert_eq!(ConfigValue::Bool(false).as_bool(), Some(false));

        assert_eq!(ConfigValue::Int(1).as_bool(), Some(true));
        assert_eq!(ConfigValue::Int(0).as_bool(), Some(false));
        assert_eq!(ConfigValue::Int(-5).as_bool(), Some(true));

        assert_eq!(ConfigValue::Float(1.0).as_bool(), Some(true));
        assert_eq!(ConfigValue::Float(0.0).as_bool(), Some(false));
        assert_eq!(ConfigValue::Float(-0.5).as_bool(), Some(true));

        assert_eq!(ConfigValue::String("yes".into()).as_bool(), Some(true));
        assert_eq!(ConfigValue::String("no".into()).as_bool(), Some(false));
        assert_eq!(ConfigValue::String("invalid".into()).as_bool(), None);

        assert_eq!(ConfigValue::Json(json!({})).as_bool(), None);
    }

    #[test]
    fn as_int_coercion() {
        assert_eq!(ConfigValue::Int(42).as_int(), Some(42));

        assert_eq!(ConfigValue::Float(3.7).as_int(), Some(3));
        assert_eq!(ConfigValue::Float(-2.9).as_int(), Some(-2));
        assert_eq!(ConfigValue::Float(f64::INFINITY).as_int(), None);
        assert_eq!(ConfigValue::Float(f64::NAN).as_int(), None);

        assert_eq!(ConfigValue::Bool(true).as_int(), Some(1));
        assert_eq!(ConfigValue::Bool(false).as_int(), Some(0));

        assert_eq!(ConfigValue::String("123".into()).as_int(), Some(123));
        assert_eq!(ConfigValue::String("-456".into()).as_int(), Some(-456));
        assert_eq!(ConfigValue::String("not a number".into()).as_int(), None);

        assert_eq!(ConfigValue::Json(json!({})).as_int(), None);
    }

    #[test]
    fn as_float_coercion() {
        assert_eq!(ConfigValue::Float(3.14).as_float(), Some(3.14));

        assert_eq!(ConfigValue::Int(42).as_float(), Some(42.0));
        assert_eq!(ConfigValue::Int(-100).as_float(), Some(-100.0));

        assert_eq!(ConfigValue::Bool(true).as_float(), Some(1.0));
        assert_eq!(ConfigValue::Bool(false).as_float(), Some(0.0));

        assert_eq!(ConfigValue::String("2.5".into()).as_float(), Some(2.5));
        assert_eq!(ConfigValue::String("-1.5".into()).as_float(), Some(-1.5));
        assert_eq!(ConfigValue::String("not a number".into()).as_float(), None);

        assert_eq!(ConfigValue::Json(json!({})).as_float(), None);
    }

    #[test]
    fn as_string_only_returns_string_variant() {
        assert_eq!(
            ConfigValue::String("hello".into()).as_string(),
            Some("hello")
        );
        assert_eq!(ConfigValue::Int(42).as_string(), None);
        assert_eq!(ConfigValue::Float(3.14).as_string(), None);
        assert_eq!(ConfigValue::Bool(true).as_string(), None);
        assert_eq!(ConfigValue::Json(json!({})).as_string(), None);
    }

    #[test]
    fn as_json_only_returns_json_variant() {
        let json_val = json!({"key": "value"});
        assert_eq!(
            ConfigValue::Json(json_val.clone()).as_json(),
            Some(&json_val)
        );
        assert_eq!(ConfigValue::String("{}".into()).as_json(), None);
        assert_eq!(ConfigValue::Int(42).as_json(), None);
    }

    // ==================== Type Name Tests ====================

    #[test]
    fn type_name_returns_correct_names() {
        assert_eq!(ConfigValue::String("".into()).type_name(), "string");
        assert_eq!(ConfigValue::Int(0).type_name(), "integer");
        assert_eq!(ConfigValue::Float(0.0).type_name(), "float");
        assert_eq!(ConfigValue::Bool(false).type_name(), "boolean");
        assert_eq!(ConfigValue::Json(json!(null)).type_name(), "json");
    }

    // ==================== Display Tests ====================

    #[test]
    fn display_formats_correctly() {
        assert_eq!(ConfigValue::String("hello".into()).to_string(), "hello");
        assert_eq!(ConfigValue::Int(42).to_string(), "42");
        assert_eq!(ConfigValue::Float(3.14).to_string(), "3.14");
        assert_eq!(ConfigValue::Bool(true).to_string(), "true");
        assert_eq!(ConfigValue::Bool(false).to_string(), "false");
        assert_eq!(ConfigValue::Json(json!({"a": 1})).to_string(), r#"{"a":1}"#);
    }

    // ==================== From Trait Tests ====================

    #[test]
    fn from_json_value() {
        assert_eq!(
            ConfigValue::from(json!(null)),
            ConfigValue::String(String::new())
        );
        assert_eq!(ConfigValue::from(json!(true)), ConfigValue::Bool(true));
        assert_eq!(ConfigValue::from(json!(42)), ConfigValue::Int(42));
        assert_eq!(ConfigValue::from(json!(3.14)), ConfigValue::Float(3.14));
        assert_eq!(
            ConfigValue::from(json!("hello")),
            ConfigValue::String("hello".into())
        );
        assert_eq!(
            ConfigValue::from(json!([1, 2, 3])),
            ConfigValue::Json(json!([1, 2, 3]))
        );
        assert_eq!(
            ConfigValue::from(json!({"key": "value"})),
            ConfigValue::Json(json!({"key": "value"}))
        );
    }

    #[test]
    fn from_primitive_types() {
        assert_eq!(
            ConfigValue::from("hello"),
            ConfigValue::String("hello".into())
        );
        assert_eq!(
            ConfigValue::from(String::from("world")),
            ConfigValue::String("world".into())
        );
        assert_eq!(ConfigValue::from(42i64), ConfigValue::Int(42));
        assert_eq!(ConfigValue::from(42i32), ConfigValue::Int(42));
        assert_eq!(ConfigValue::from(3.14f64), ConfigValue::Float(3.14));
        assert_eq!(ConfigValue::from(true), ConfigValue::Bool(true));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn parse_edge_cases() {
        // Empty string
        assert_eq!(
            ConfigValue::parse("", None),
            ConfigValue::String(String::new())
        );

        // Whitespace in string (not trimmed)
        assert_eq!(
            ConfigValue::parse("  ", None),
            ConfigValue::String("  ".into())
        );

        // Negative zero
        assert_eq!(ConfigValue::parse("-0", None), ConfigValue::Int(0));
        assert_eq!(ConfigValue::parse("-0.0", None), ConfigValue::Float(-0.0));

        // Scientific notation
        assert_eq!(
            ConfigValue::parse("1e5", None),
            ConfigValue::Float(100000.0)
        );
        assert_eq!(ConfigValue::parse("1E-3", None), ConfigValue::Float(0.001));

        // Leading zeros (parsed as decimal, not octal)
        assert_eq!(ConfigValue::parse("007", None), ConfigValue::Int(7));
    }

    #[test]
    fn clone_and_debug() {
        let val = ConfigValue::String("test".into());
        let cloned = val.clone();
        assert_eq!(val, cloned);

        // Debug should not panic
        let debug = format!("{:?}", val);
        assert!(debug.contains("String"));
    }
}
