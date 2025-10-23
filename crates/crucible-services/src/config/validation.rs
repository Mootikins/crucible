//! Enhanced configuration validation and error handling for Crucible services
//!
//! This module provides comprehensive configuration validation with detailed error reporting,
//! context information, and graceful error recovery strategies for the Phase 7.3 implementation.

use crate::errors::ServiceResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Comprehensive configuration validation errors
#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    /// Missing required configuration field
    #[error("Missing required field: {field}")]
    MissingField {
        field: String,
        context: ValidationContext,
    },

    /// Invalid value for configuration field
    #[error("Invalid value for field '{field}': {value} - {reason}")]
    InvalidValue {
        field: String,
        value: String,
        reason: String,
        context: ValidationContext,
        suggested_fix: Option<String>,
    },

    /// Configuration dependency violation
    #[error("Configuration dependency violation: {message}")]
    DependencyViolation {
        message: String,
        field: String,
        depends_on: String,
        context: ValidationContext,
    },

    /// Configuration type mismatch
    #[error("Type mismatch for field '{field}': expected {expected_type}, got {actual_type}")]
    TypeMismatch {
        field: String,
        expected_type: String,
        actual_type: String,
        context: ValidationContext,
    },

    /// Configuration constraint violation
    #[error("Constraint violation for field '{field}': {constraint} ({details})")]
    ConstraintViolation {
        field: String,
        constraint: String,
        details: String,
        context: ValidationContext,
    },

    /// Configuration file parsing error
    #[error("Configuration parsing error in {file_source}: {error}")]
    ParseError {
        file_source: String,
        error: String,
        line: Option<usize>,
        column: Option<usize>,
    },

    /// Environment variable configuration error
    #[error("Environment variable error: {variable} - {error}")]
    EnvironmentError {
        variable: String,
        error: String,
        env_source: Option<String>,
    },

    /// Multiple validation errors occurred
    #[error("Multiple validation errors occurred ({count} errors)")]
    MultipleErrors {
        count: usize,
        errors: Vec<ValidationError>,
    },
}

impl ValidationError {
    /// Get the validation context
    pub fn context(&self) -> Option<&ValidationContext> {
        match self {
            Self::MissingField { context, .. }
            | Self::InvalidValue { context, .. }
            | Self::DependencyViolation { context, .. }
            | Self::TypeMismatch { context, .. }
            | Self::ConstraintViolation { context, .. } => Some(context),
            _ => None,
        }
    }

    /// Get the field name if applicable
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::MissingField { field, .. }
            | Self::InvalidValue { field, .. }
            | Self::DependencyViolation { field, .. }
            | Self::TypeMismatch { field, .. }
            | Self::ConstraintViolation { field, .. } => Some(field),
            _ => None,
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::InvalidValue { suggested_fix: Some(_), .. } => true,
            Self::InvalidValue { suggested_fix: None, .. } => false,
            Self::EnvironmentError { .. } => true,
            Self::MissingField { .. } => false,
            Self::DependencyViolation { .. } => false,
            Self::TypeMismatch { .. } => false,
            Self::ConstraintViolation { .. } => false,
            Self::ParseError { .. } => false,
            Self::MultipleErrors { .. } => false,
        }
    }

    /// Get error severity
    pub fn severity(&self) -> ValidationSeverity {
        match self {
            Self::MissingField { .. } => ValidationSeverity::Error,
            Self::InvalidValue { .. } => ValidationSeverity::Error,
            Self::DependencyViolation { .. } => ValidationSeverity::Error,
            Self::TypeMismatch { .. } => ValidationSeverity::Error,
            Self::ConstraintViolation { .. } => ValidationSeverity::Warning,
            Self::ParseError { .. } => ValidationSeverity::Error,
            Self::EnvironmentError { .. } => ValidationSeverity::Warning,
            Self::MultipleErrors { errors, .. } => {
                // If any error is severe, mark as Error
                if errors.iter().any(|e| e.severity() == ValidationSeverity::Error) {
                    ValidationSeverity::Error
                } else {
                    ValidationSeverity::Warning
                }
            }
        }
    }

    /// Get human-readable error description with suggestions
    pub fn description(&self) -> String {
        match self {
            Self::MissingField { field, context } => {
                format!(
                    "The configuration field '{}' is required but was not found in {}. Please add this field to your configuration.",
                    field,
                    context.source
                )
            }
            Self::InvalidValue { field, value, reason, suggested_fix, .. } => {
                let mut desc = format!(
                    "The value '{}' for field '{}' is invalid: {}. Please provide a valid value.",
                    value, field, reason
                );
                if let Some(fix) = suggested_fix {
                    desc.push_str(&format!(" Suggested fix: {}", fix));
                }
                desc
            }
            Self::DependencyViolation { field, depends_on, message, .. } => {
                format!(
                    "Field '{}' depends on '{}' but there's a dependency issue: {}. Please ensure both fields are properly configured.",
                    field, depends_on, message
                )
            }
            Self::TypeMismatch { field, expected_type, actual_type, .. } => {
                format!(
                    "Field '{}' expects a value of type {} but received {}. Please check your configuration.",
                    field, expected_type, actual_type
                )
            }
            Self::ConstraintViolation { field, constraint, details, .. } => {
                format!(
                    "Field '{}' violates the '{}' constraint: {}. Please adjust the value.",
                    field, constraint, details
                )
            }
            Self::ParseError { file_source, error, line, column } => {
                let mut desc = format!("Failed to parse configuration from {}: {}", file_source, error);
                if let Some(line_num) = line {
                    desc.push_str(&format!(" (line {})", line_num));
                }
                if let Some(col) = column {
                    desc.push_str(&format!(" (column {})", col));
                }
                desc
            }
            Self::EnvironmentError { variable, error, env_source } => {
                let mut desc = format!("Environment variable '{}' error: {}", variable, error);
                if let Some(src) = env_source {
                    desc.push_str(&format!(" (source: {})", src));
                }
                desc
            }
            Self::MultipleErrors { count, errors: _ } => {
                format!(
                    "Multiple validation errors occurred ({} total). See individual error details for more information.",
                    count
                )
            }
        }
    }
}

/// Validation severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationSeverity {
    /// Informational message
    Info,
    /// Warning - configuration may work but not optimal
    Warning,
    /// Error - configuration will not work
    Error,
    /// Critical - system cannot start
    Critical,
}

impl fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Validation context for error reporting
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationContext {
    /// Source of the configuration (file, env, default)
    pub source: String,
    /// Configuration section
    pub section: Option<String>,
    /// Nested field path
    pub field_path: Vec<String>,
    /// Additional context information
    pub metadata: HashMap<String, String>,
}

impl ValidationContext {
    /// Create new validation context
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            section: None,
            field_path: vec![],
            metadata: HashMap::new(),
        }
    }

    /// Add section to context
    pub fn with_section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    /// Add field path component
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field_path.push(field.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get full field path as string
    pub fn full_field_path(&self) -> String {
        if self.field_path.is_empty() {
            "root".to_string()
        } else {
            self.field_path.join(".")
        }
    }
}

/// Validation result with multiple errors
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// Validation errors encountered
    pub errors: Vec<ValidationError>,
    /// Validation warnings encountered
    pub warnings: Vec<ValidationError>,
    /// Additional validation information
    pub info: Vec<String>,
}

impl ValidationResult {
    /// Create successful validation result
    pub fn success() -> Self {
        Self {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
            info: vec![],
        }
    }

    /// Create validation result with errors
    pub fn with_errors(errors: Vec<ValidationError>) -> Self {
        let is_valid = errors.is_empty();
        let mut warnings = vec![];
        let mut actual_errors = vec![];

        for error in errors {
            match error.severity() {
                ValidationSeverity::Warning => warnings.push(error),
                _ => actual_errors.push(error),
            }
        }

        Self {
            is_valid: is_valid && actual_errors.is_empty(),
            errors: actual_errors,
            warnings,
            info: vec![],
        }
    }

    /// Add warning to result
    pub fn with_warning(mut self, warning: ValidationError) -> Self {
        if warning.severity() == ValidationSeverity::Warning {
            self.warnings.push(warning);
        } else {
            self.errors.push(warning);
            self.is_valid = false;
        }
        self
    }

    /// Add info message
    pub fn with_info(mut self, info: impl Into<String>) -> Self {
        self.info.push(info.into());
        self
    }

    /// Add error to result
    pub fn with_error(mut self, error: ValidationError) -> Self {
        if error.severity() == ValidationSeverity::Warning {
            self.warnings.push(error);
        } else {
            self.errors.push(error);
            self.is_valid = false;
        }
        self
    }

    /// Check if there are any issues (errors or warnings)
    pub fn has_issues(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }

    /// Log validation result
    pub fn log_result(&self, component: &str) {
        if self.is_valid && !self.has_issues() {
            info!(component = %component, "Configuration validation passed");
            return;
        }

        if !self.errors.is_empty() {
            error!(
                component = %component,
                error_count = %self.errors.len(),
                "Configuration validation failed"
            );
            for error in &self.errors {
                error!(component = %component, field = %error.field().unwrap_or("unknown"), error = %error, "Validation error");
            }
        }

        if !self.warnings.is_empty() {
            warn!(
                component = %component,
                warning_count = %self.warnings.len(),
                "Configuration validation warnings"
            );
            for warning in &self.warnings {
                warn!(component = %component, field = %warning.field().unwrap_or("unknown"), warning = %warning, "Validation warning");
            }
        }

        for info_msg in &self.info {
            debug!(component = %component, message = %info_msg, "Validation info");
        }
    }

    /// Convert to ServiceResult
    pub fn into_service_result(self) -> ServiceResult<()> {
        if self.is_valid {
            Ok(())
        } else {
            Err(crate::errors::ServiceError::ValidationError(
                self.errors
                    .into_iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        }
    }
}

/// Configuration validation trait
pub trait ConfigValidator {
    /// Validate configuration
    fn validate(&self) -> ValidationResult;

    /// Validate with context
    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult;

    /// Get validation rules for this configuration
    fn validation_rules(&self) -> Vec<ValidationRule>;
}

/// Validation rule definition
#[derive(Debug, Clone)]
pub struct ValidationRule {
    /// Field name this rule applies to
    pub field: String,
    /// Rule type
    pub rule_type: ValidationRuleType,
    /// Rule parameters
    pub parameters: HashMap<String, String>,
    /// Error message template
    pub error_message: String,
    /// Whether this rule is required or optional
    pub required: bool,
}

/// Types of validation rules
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationRuleType {
    /// Field must be present
    Required,
    /// Field must match regex pattern
    Pattern(String),
    /// Field must be within range
    Range { min: Option<f64>, max: Option<f64> },
    /// Field must be one of allowed values
    Enum(Vec<String>),
    /// Field must be valid URL
    Url,
    /// Field must be valid email
    Email,
    /// Field must be valid file path
    FilePath,
    /// Field must be positive number
    Positive,
    /// Field must be non-empty string
    NonEmpty,
    /// Custom validation function
    Custom(String),
}

/// Validation engine for processing validation rules
pub struct ValidationEngine {
    pub rules: HashMap<String, Vec<ValidationRule>>,
}

impl ValidationEngine {
    /// Create new validation engine
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// Add validation rule for a field
    pub fn add_rule(&mut self, field: impl Into<String>, rule: ValidationRule) {
        self.rules.entry(field.into()).or_default().push(rule);
    }

    /// Add multiple rules for a field
    pub fn add_rules(&mut self, field: impl Into<String> + Clone, rules: Vec<ValidationRule>) {
        for rule in rules {
            self.add_rule(field.clone(), rule);
        }
    }

    /// Validate configuration value against rules
    pub fn validate_field(
        &self,
        field: &str,
        value: &serde_json::Value,
        context: &ValidationContext,
    ) -> ValidationResult {
        let mut result = ValidationResult::success();

        if let Some(rules) = self.rules.get(field) {
            for rule in rules {
                if let Err(error) = self.apply_rule(field, value, rule, context) {
                    result = result.with_error(error);
                }
            }
        }

        result
    }

    /// Apply single validation rule
    fn apply_rule(
        &self,
        field: &str,
        value: &serde_json::Value,
        rule: &ValidationRule,
        context: &ValidationContext,
    ) -> Result<(), ValidationError> {
        match &rule.rule_type {
            ValidationRuleType::Required => {
                if value.is_null() {
                    return Err(ValidationError::MissingField {
                        field: field.to_string(),
                        context: context.clone(),
                    });
                }
            }
            ValidationRuleType::Pattern(pattern) => {
                if let Some(s) = value.as_str() {
                    let regex = regex::Regex::new(pattern).map_err(|e| {
                        ValidationError::InvalidValue {
                            field: field.to_string(),
                            value: s.to_string(),
                            reason: format!("Invalid regex pattern: {}", e),
                            context: context.clone(),
                            suggested_fix: None,
                        }
                    })?;
                    if !regex.is_match(s) {
                        return Err(ValidationError::InvalidValue {
                            field: field.to_string(),
                            value: s.to_string(),
                            reason: rule.error_message.clone(),
                            context: context.clone(),
                            suggested_fix: Some(format!("Value must match pattern: {}", pattern)),
                        });
                    }
                }
            }
            ValidationRuleType::Range { min, max } => {
                if let Some(num) = value.as_f64() {
                    if let Some(min_val) = min {
                        if num < *min_val {
                            return Err(ValidationError::ConstraintViolation {
                                field: field.to_string(),
                                constraint: "minimum value".to_string(),
                                details: format!("Value {} is less than minimum {}", num, min_val),
                                context: context.clone(),
                            });
                        }
                    }
                    if let Some(max_val) = max {
                        if num > *max_val {
                            return Err(ValidationError::ConstraintViolation {
                                field: field.to_string(),
                                constraint: "maximum value".to_string(),
                                details: format!("Value {} is greater than maximum {}", num, max_val),
                                context: context.clone(),
                            });
                        }
                    }
                }
            }
            ValidationRuleType::Enum(allowed_values) => {
                if let Some(s) = value.as_str() {
                    if !allowed_values.contains(&s.to_string()) {
                        return Err(ValidationError::InvalidValue {
                            field: field.to_string(),
                            value: s.to_string(),
                            reason: format!("Value must be one of: {}", allowed_values.join(", ")),
                            context: context.clone(),
                            suggested_fix: Some(format!("Choose from: {}", allowed_values.join(", "))),
                        });
                    }
                }
            }
            ValidationRuleType::NonEmpty => {
                if let Some(s) = value.as_str() {
                    if s.trim().is_empty() {
                        return Err(ValidationError::InvalidValue {
                            field: field.to_string(),
                            value: s.to_string(),
                            reason: "Value cannot be empty".to_string(),
                            context: context.clone(),
                            suggested_fix: Some("Provide a non-empty value".to_string()),
                        });
                    }
                }
            }
            ValidationRuleType::Positive => {
                if let Some(num) = value.as_f64() {
                    if num <= 0.0 {
                        return Err(ValidationError::ConstraintViolation {
                            field: field.to_string(),
                            constraint: "positive value".to_string(),
                            details: format!("Value {} must be greater than 0", num),
                            context: context.clone(),
                        });
                    }
                }
            }
            _ => {
                // For URL, Email, FilePath, Custom - implement as needed
                debug!(field = %field, rule_type = ?rule.rule_type, "Validation rule not yet implemented");
            }
        }

        Ok(())
    }

    /// Validate entire configuration object
    pub fn validate_config(
        &self,
        config: &serde_json::Value,
        context: &ValidationContext,
    ) -> ValidationResult {
        let mut result = ValidationResult::success();

        if let Some(obj) = config.as_object() {
            for (field, value) in obj {
                let field_result = self.validate_field(field, value, context);
                if !field_result.is_valid {
                    result.is_valid = false;
                    result.errors.extend(field_result.errors);
                }
                result.warnings.extend(field_result.warnings);
                result.info.extend(field_result.info);
            }
        }

        result
    }
}

impl Default for ValidationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_context() {
        let context = ValidationContext::new("test.yaml")
            .with_section("logging")
            .with_field("level")
            .with_metadata("source", "file");

        assert_eq!(context.source, "test.yaml");
        assert_eq!(context.section, Some("logging".to_string()));
        assert_eq!(context.full_field_path(), "level");
        assert_eq!(context.metadata.get("source"), Some(&"file".to_string()));
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::success();
        assert!(result.is_valid);

        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        result = result.with_error(error);
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_validation_engine() {
        let mut engine = ValidationEngine::new();
        let rule = ValidationRule {
            field: "test_field".to_string(),
            rule_type: ValidationRuleType::Required,
            parameters: HashMap::new(),
            error_message: "Field is required".to_string(),
            required: true,
        };

        engine.add_rule("test_field", rule);

        let config = serde_json::json!({
            "test_field": "value"
        });

        let context = ValidationContext::new("test");
        let result = engine.validate_config(&config, &context);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validation_error_description() {
        let error = ValidationError::MissingField {
            field: "required_field".to_string(),
            context: ValidationContext::new("config.yaml").with_section("database"),
        };

        let description = error.description();
        assert!(description.contains("required_field"));
        assert!(description.contains("config.yaml"));
    }
}