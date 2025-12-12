//! TOON validation and error categorization
//!
//! Uses the toon-format crate for parsing and provides error categorization.

use serde_json::Value;
use std::fmt;

/// Error categories for TOON evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToonError {
    /// toon-format parse failure
    InvalidSyntax(String),

    /// Parses but has wrong structure
    MissingField(String),
    ExtraField(String),
    WrongType {
        field: String,
        expected: String,
        got: String,
    },
    WrongArrayLength {
        expected: usize,
        got: usize,
    },

    /// Right structure, wrong values
    ValueMismatch {
        path: String,
        expected: String,
        got: String,
    },

    /// Empty or whitespace-only response
    EmptyResponse,

    /// Response contained non-TOON content (explanation, markdown, etc.)
    ContainsNonToon(String),
}

impl fmt::Display for ToonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToonError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
            ToonError::MissingField(field) => write!(f, "Missing field: {}", field),
            ToonError::ExtraField(field) => write!(f, "Extra field: {}", field),
            ToonError::WrongType {
                field,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Wrong type for '{}': expected {}, got {}",
                    field, expected, got
                )
            }
            ToonError::WrongArrayLength { expected, got } => {
                write!(f, "Wrong array length: expected {}, got {}", expected, got)
            }
            ToonError::ValueMismatch {
                path,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Value mismatch at '{}': expected {}, got {}",
                    path, expected, got
                )
            }
            ToonError::EmptyResponse => write!(f, "Empty response"),
            ToonError::ContainsNonToon(sample) => {
                write!(f, "Response contains non-TOON content: {}...", sample)
            }
        }
    }
}

/// Result of validating TOON output
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub success: bool,
    /// The parsed JSON value (if parsing succeeded)
    pub parsed: Option<Value>,
    /// List of errors found
    pub errors: Vec<ToonError>,
    /// Raw response from LLM
    pub raw_response: String,
}

impl ValidationResult {
    /// Create a successful result
    pub fn success(parsed: Value, raw: String) -> Self {
        Self {
            success: true,
            parsed: Some(parsed),
            errors: vec![],
            raw_response: raw,
        }
    }

    /// Create a failed result
    pub fn failure(errors: Vec<ToonError>, raw: String) -> Self {
        Self {
            success: false,
            parsed: None,
            errors,
            raw_response: raw,
        }
    }

    /// Create a failed result with parsed value (structural errors)
    pub fn partial(parsed: Value, errors: Vec<ToonError>, raw: String) -> Self {
        Self {
            success: false,
            parsed: Some(parsed),
            errors,
            raw_response: raw,
        }
    }
}

/// Extract TOON content from LLM response
///
/// Handles common cases:
/// - Raw TOON output
/// - TOON wrapped in ```toon code blocks
/// - TOON with leading/trailing explanation
pub fn extract_toon(response: &str) -> Result<String, ToonError> {
    let trimmed = response.trim();

    if trimmed.is_empty() {
        return Err(ToonError::EmptyResponse);
    }

    // Check for code block
    if let Some(start) = trimmed.find("```toon") {
        let content_start = start + 7; // len("```toon")
        if let Some(end) = trimmed[content_start..].find("```") {
            let toon = trimmed[content_start..content_start + end].trim();
            return Ok(toon.to_string());
        }
    }

    // Check for generic code block
    if let Some(start) = trimmed.find("```\n") {
        let content_start = start + 4;
        if let Some(end) = trimmed[content_start..].find("```") {
            let toon = trimmed[content_start..content_start + end].trim();
            return Ok(toon.to_string());
        }
    }

    // Check for obvious non-TOON indicators
    let lower = trimmed.to_lowercase();
    if lower.starts_with("here") || lower.starts_with("the ") || lower.starts_with("this ") {
        // Likely contains explanation, try to find TOON after it
        // Look for first line that looks like TOON (key: value or array header)
        for (i, line) in trimmed.lines().enumerate() {
            let line = line.trim();
            if line.contains(": ") || line.contains("[") && line.contains("]:") {
                // Found potential TOON start
                let toon: String = trimmed.lines().skip(i).collect::<Vec<_>>().join("\n");
                return Ok(toon);
            }
        }
        return Err(ToonError::ContainsNonToon(
            trimmed.chars().take(50).collect(),
        ));
    }

    // Assume raw TOON
    Ok(trimmed.to_string())
}

/// Extract JSON content from LLM response
pub fn extract_json(response: &str) -> Result<String, ToonError> {
    let trimmed = response.trim();

    if trimmed.is_empty() {
        return Err(ToonError::EmptyResponse);
    }

    // Check for code block
    if let Some(start) = trimmed.find("```json") {
        let content_start = start + 7;
        if let Some(end) = trimmed[content_start..].find("```") {
            let json = trimmed[content_start..content_start + end].trim();
            return Ok(json.to_string());
        }
    }

    // Check for generic code block
    if let Some(start) = trimmed.find("```\n") {
        let content_start = start + 4;
        if let Some(end) = trimmed[content_start..].find("```") {
            let json = trimmed[content_start..content_start + end].trim();
            return Ok(json.to_string());
        }
    }

    // Try to find JSON object/array boundaries
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return Ok(trimmed[start..=end].to_string());
        }
    }
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            return Ok(trimmed[start..=end].to_string());
        }
    }

    // Assume raw JSON
    Ok(trimmed.to_string())
}

/// Validate TOON output against expected JSON
pub fn validate_toon_output(llm_response: &str, expected_json: &Value) -> ValidationResult {
    // Extract TOON from response
    let toon = match extract_toon(llm_response) {
        Ok(t) => t,
        Err(e) => return ValidationResult::failure(vec![e], llm_response.to_string()),
    };

    // Parse TOON to JSON using toon-format
    let parsed: Value = match toon_format::decode_default(&toon) {
        Ok(v) => v,
        Err(e) => {
            return ValidationResult::failure(
                vec![ToonError::InvalidSyntax(e.to_string())],
                llm_response.to_string(),
            );
        }
    };

    // Compare structures
    let errors = compare_json_values(&parsed, expected_json, "");

    if errors.is_empty() {
        ValidationResult::success(parsed, llm_response.to_string())
    } else {
        ValidationResult::partial(parsed, errors, llm_response.to_string())
    }
}

/// Validate JSON output against expected JSON
pub fn validate_json_output(llm_response: &str, expected_json: &Value) -> ValidationResult {
    // Extract JSON from response
    let json_str = match extract_json(llm_response) {
        Ok(j) => j,
        Err(e) => return ValidationResult::failure(vec![e], llm_response.to_string()),
    };

    // Parse JSON
    let parsed: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            return ValidationResult::failure(
                vec![ToonError::InvalidSyntax(e.to_string())],
                llm_response.to_string(),
            );
        }
    };

    // Compare structures
    let errors = compare_json_values(&parsed, expected_json, "");

    if errors.is_empty() {
        ValidationResult::success(parsed, llm_response.to_string())
    } else {
        ValidationResult::partial(parsed, errors, llm_response.to_string())
    }
}

/// Compare two JSON values and return list of differences
fn compare_json_values(got: &Value, expected: &Value, path: &str) -> Vec<ToonError> {
    let mut errors = Vec::new();

    match (got, expected) {
        (Value::Object(got_obj), Value::Object(exp_obj)) => {
            // Check for missing fields
            for key in exp_obj.keys() {
                if !got_obj.contains_key(key) {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    errors.push(ToonError::MissingField(field_path));
                }
            }

            // Check for extra fields
            for key in got_obj.keys() {
                if !exp_obj.contains_key(key) {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    errors.push(ToonError::ExtraField(field_path));
                }
            }

            // Recursively compare common fields
            for (key, exp_val) in exp_obj {
                if let Some(got_val) = got_obj.get(key) {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    errors.extend(compare_json_values(got_val, exp_val, &field_path));
                }
            }
        }

        (Value::Array(got_arr), Value::Array(exp_arr)) => {
            if got_arr.len() != exp_arr.len() {
                errors.push(ToonError::WrongArrayLength {
                    expected: exp_arr.len(),
                    got: got_arr.len(),
                });
            }

            // Compare elements up to shorter length
            for (i, (got_elem, exp_elem)) in got_arr.iter().zip(exp_arr.iter()).enumerate() {
                let elem_path = format!("{}[{}]", path, i);
                errors.extend(compare_json_values(got_elem, exp_elem, &elem_path));
            }
        }

        // Type mismatches
        (got, expected) if std::mem::discriminant(got) != std::mem::discriminant(expected) => {
            errors.push(ToonError::WrongType {
                field: path.to_string(),
                expected: json_type_name(expected).to_string(),
                got: json_type_name(got).to_string(),
            });
        }

        // Value comparisons for primitives
        (Value::String(got_s), Value::String(exp_s)) if got_s != exp_s => {
            errors.push(ToonError::ValueMismatch {
                path: path.to_string(),
                expected: format!("\"{}\"", exp_s),
                got: format!("\"{}\"", got_s),
            });
        }

        (Value::Number(got_n), Value::Number(exp_n)) if got_n != exp_n => {
            errors.push(ToonError::ValueMismatch {
                path: path.to_string(),
                expected: exp_n.to_string(),
                got: got_n.to_string(),
            });
        }

        (Value::Bool(got_b), Value::Bool(exp_b)) if got_b != exp_b => {
            errors.push(ToonError::ValueMismatch {
                path: path.to_string(),
                expected: exp_b.to_string(),
                got: got_b.to_string(),
            });
        }

        // Null matches null, other primitives match if equal
        _ => {}
    }

    errors
}

/// Get human-readable type name for JSON value
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_toon_raw() {
        let input = "name: Ada\nage: 30";
        assert_eq!(extract_toon(input).unwrap(), "name: Ada\nage: 30");
    }

    #[test]
    fn test_extract_toon_code_block() {
        let input = "Here's the TOON:\n```toon\nname: Ada\n```";
        assert_eq!(extract_toon(input).unwrap(), "name: Ada");
    }

    #[test]
    fn test_extract_json_raw() {
        let input = r#"{"name": "Ada"}"#;
        assert_eq!(extract_json(input).unwrap(), r#"{"name": "Ada"}"#);
    }

    #[test]
    fn test_extract_json_with_explanation() {
        let input = "Here is the JSON:\n{\"name\": \"Ada\"}";
        assert_eq!(extract_json(input).unwrap(), r#"{"name": "Ada"}"#);
    }

    #[test]
    fn test_compare_matching_objects() {
        let a = json!({"name": "Ada", "age": 30});
        let b = json!({"name": "Ada", "age": 30});
        assert!(compare_json_values(&a, &b, "").is_empty());
    }

    #[test]
    fn test_compare_missing_field() {
        let got = json!({"name": "Ada"});
        let expected = json!({"name": "Ada", "age": 30});
        let errors = compare_json_values(&got, &expected, "");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ToonError::MissingField(ref f) if f == "age"));
    }

    #[test]
    fn test_compare_extra_field() {
        let got = json!({"name": "Ada", "extra": true});
        let expected = json!({"name": "Ada"});
        let errors = compare_json_values(&got, &expected, "");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ToonError::ExtraField(ref f) if f == "extra"));
    }

    #[test]
    fn test_compare_wrong_type() {
        let got = json!({"age": "30"});
        let expected = json!({"age": 30});
        let errors = compare_json_values(&got, &expected, "");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ToonError::WrongType { .. }));
    }

    #[test]
    fn test_compare_nested() {
        let got = json!({"user": {"name": "Bob"}});
        let expected = json!({"user": {"name": "Ada"}});
        let errors = compare_json_values(&got, &expected, "");
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(errors[0], ToonError::ValueMismatch { ref path, .. } if path == "user.name")
        );
    }
}
