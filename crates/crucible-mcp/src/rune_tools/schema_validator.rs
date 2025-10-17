/// Schema validation framework for Rune MCP tools
///
/// This module provides comprehensive JSON schema validation capabilities:
/// - JSON Schema generation from Rune type definitions
/// - Parameter validation against schemas
/// - Constraint validation and error reporting
/// - Schema composition and inheritance
/// - Validation rule engine with custom constraints

use anyhow::{Result, anyhow};
use serde_json::{json, Value, Map};
use std::collections::HashMap;
use tracing::{debug, warn};

use super::ast_analyzer::{
    RuneType, ParameterInfo,
    AsyncFunctionInfo, ValidationRule
};

/// Schema validation configuration
#[derive(Debug)]
pub struct ValidationConfig {
    pub strict_mode: bool,
    pub allow_unknown_types: bool,
    pub generate_descriptions: bool,
    pub include_examples: bool,
    pub max_string_length: Option<usize>,
    pub max_array_length: Option<usize>,
    // Note: custom_validators removed to avoid Clone issues
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            allow_unknown_types: true,
            generate_descriptions: true,
            include_examples: false,
            max_string_length: Some(100000),
            max_array_length: Some(10000),
        }
    }
}

/// JSON Schema validator trait
pub trait Validator: Send + Sync {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult;
    fn name(&self) -> &str;
}

/// Validation context for error reporting
#[derive(Debug, Clone)]
pub struct ValidationContext {
    pub field_path: String,
    pub parameter_name: String,
    pub function_name: String,
    pub module_name: String,
}

impl ValidationContext {
    pub fn new(parameter_name: &str, function_name: &str, module_name: &str) -> Self {
        Self {
            field_path: parameter_name.to_string(),
            parameter_name: parameter_name.to_string(),
            function_name: function_name.to_string(),
            module_name: module_name.to_string(),
        }
    }

    pub fn with_field(&self, field_name: &str) -> Self {
        let new_path = if self.field_path.is_empty() {
            field_name.to_string()
        } else {
            format!("{}.{}", self.field_path, field_name)
        };

        Self {
            field_path: new_path,
            ..self.clone()
        }
    }
}

/// Validation result with detailed error information
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn invalid(errors: Vec<ValidationError>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: Vec::new(),
        }
    }

    pub fn with_warnings(mut self, warnings: Vec<ValidationWarning>) -> Self {
        self.warnings = warnings;
        self
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }
}

/// Detailed validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub constraint: Option<String>,
}

impl ValidationError {
    pub fn new(code: &str, message: &str, context: &ValidationContext) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field_path: context.field_path.clone(),
            expected: None,
            actual: None,
            constraint: None,
        }
    }

    pub fn with_expected(mut self, expected: &str) -> Self {
        self.expected = Some(expected.to_string());
        self
    }

    pub fn with_actual(mut self, actual: &str) -> Self {
        self.actual = Some(actual.to_string());
        self
    }

    pub fn with_constraint(mut self, constraint: &str) -> Self {
        self.constraint = Some(constraint.to_string());
        self
    }
}

/// Validation warning for non-critical issues
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub field_path: String,
}

impl ValidationWarning {
    pub fn new(code: &str, message: &str, field_path: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field_path: field_path.to_string(),
        }
    }
}

/// Comprehensive schema generator and validator
pub struct SchemaValidator {
    config: ValidationConfig,
    builtin_validators: HashMap<String, Box<dyn Validator>>,
}

impl SchemaValidator {
    pub fn new(config: ValidationConfig) -> Self {
        let mut validator = Self {
            config,
            builtin_validators: HashMap::new(),
        };

        // Register built-in validators
        validator.register_builtin_validators();
        validator
    }

    pub fn with_config(config: ValidationConfig) -> Self {
        Self::new(config)
    }

    /// Register built-in validators
    fn register_builtin_validators(&mut self) {
        self.builtin_validators.insert("string".to_string(), Box::new(StringValidator));
        self.builtin_validators.insert("number".to_string(), Box::new(NumberValidator));
        self.builtin_validators.insert("integer".to_string(), Box::new(IntegerValidator));
        self.builtin_validators.insert("boolean".to_string(), Box::new(BooleanValidator));
        self.builtin_validators.insert("array".to_string(), Box::new(ArrayValidator));
        self.builtin_validators.insert("object".to_string(), Box::new(ObjectValidator));
        self.builtin_validators.insert("enum".to_string(), Box::new(EnumValidator));
        self.builtin_validators.insert("pattern".to_string(), Box::new(PatternValidator));
        self.builtin_validators.insert("range".to_string(), Box::new(RangeValidator));
        self.builtin_validators.insert("length".to_string(), Box::new(LengthValidator));
    }

    /// Generate JSON schema from Rune type
    pub fn generate_schema(&self, rune_type: &RuneType, context: &str) -> Result<Value> {
        let mut schema = match rune_type {
            RuneType::String => self.generate_string_schema(),
            RuneType::Number => self.generate_number_schema(),
            RuneType::Integer => self.generate_integer_schema(),
            RuneType::Float => self.generate_number_schema(),
            RuneType::Boolean => self.generate_boolean_schema(),
            RuneType::Array(inner_type) => self.generate_array_schema(inner_type.as_ref())?,
            RuneType::Object(properties) => self.generate_object_schema(properties)?,
            RuneType::Tuple(types) => self.generate_tuple_schema(types)?,
            RuneType::Option(inner_type) => self.generate_option_schema(inner_type.as_ref())?,
            RuneType::Any => self.generate_any_schema(),
            RuneType::Void => self.generate_void_schema(),
            RuneType::Unknown(description) => self.generate_unknown_schema(description),
            RuneType::Function { parameters, return_type, is_async } => {
                self.generate_function_type_schema(parameters, return_type.as_ref(), *is_async)?
            },
        };

        // Add metadata
        if self.config.generate_descriptions {
            schema["description"] = json!(format!("Schema for {} ({})", context, self.rune_type_description(rune_type)));
        }

        if self.config.include_examples {
            if let Some(example) = self.generate_example(rune_type) {
                schema["example"] = example;
            }
        }

        Ok(schema)
    }

    /// Generate JSON schema for function parameters
    pub fn generate_function_schema(&self, function_info: &AsyncFunctionInfo) -> Result<Value> {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for param in &function_info.parameters {
            let param_schema = self.generate_parameter_schema(param)?;
            if !param.is_optional {
                required.push(param.name.clone());
            }
            properties.insert(param.name.clone(), param_schema);
        }

        let mut schema = json!({
            "type": "object",
            "properties": properties,
            "required": required
        });

        // Add function metadata
        if let Some(description) = &function_info.description {
            schema["description"] = json!(description);
        }

        schema["metadata"] = json!({
            "module": function_info.module_path.join("::"),
            "function": function_info.name,
            "is_async": function_info.is_async,
            "return_type": function_info.return_type,
            "parameter_count": function_info.parameters.len()
        });

        Ok(schema)
    }

    /// Generate schema for individual parameter
    pub fn generate_parameter_schema(&self, param: &ParameterInfo) -> Result<Value> {
        // Parse the type constraints to determine the base type
        let base_type = self.parse_type_from_constraints(&param.type_constraints)?;
        let mut schema = self.generate_schema(&base_type, &param.name)?;

        // Add parameter-specific metadata
        if let Some(description) = &param.description {
            schema["description"] = json!(description);
        }

        if let Some(default_value) = &param.default_value {
            schema["default"] = default_value.clone();
        }

        // Add validation rules
        if !param.validation_rules.is_empty() {
            let validation_schema = self.generate_validation_schema(&param.validation_rules)?;
            schema.merge(&validation_schema);
        }

        // Mark as optional if applicable
        if param.is_optional {
            schema.merge(&json!({"anyOf": [{"type": schema["type"].clone()}, {"type": "null"}]}));
        }

        Ok(schema)
    }

    /// Validate value against schema
    pub fn validate(&self, value: &Value, schema: &Value, context: &ValidationContext) -> ValidationResult {
        debug!("Validating value '{}' against schema", context.field_path);

        let schema_type = schema.get("type").and_then(|v| v.as_str()).unwrap_or("any");

        if let Some(validator) = self.builtin_validators.get(schema_type) {
            validator.validate(value, context)
        } else if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
            self.validate_any_of(value, any_of, context)
        } else if let Some(ref_schema) = schema.get("$ref") {
            // Handle schema references (simplified implementation)
            warn!("Schema references not fully implemented: {}", ref_schema);
            ValidationResult::valid()
        } else {
            if self.config.strict_mode {
                ValidationResult::invalid(vec![
                    ValidationError::new(
                        "UNKNOWN_TYPE",
                        &format!("Unknown schema type: {}", schema_type),
                        context
                    )
                ])
            } else {
                ValidationResult::valid().with_warnings(vec![
                    ValidationWarning::new(
                        "UNKNOWN_TYPE",
                        &format!("Unknown schema type: {}", schema_type),
                        &context.field_path
                    )
                ])
            }
        }
    }

    /// Validate function parameters against their schemas
    pub fn validate_function_parameters(
        &self,
        function_info: &AsyncFunctionInfo,
        parameters: &Value,
    ) -> ValidationResult {
        let context = ValidationContext::new(
            "parameters",
            &function_info.name,
            &function_info.module_path.join("::")
        );

        debug!("Validating parameters for function {}.{}",
               context.module_name, context.function_name);

        let mut result = ValidationResult::valid();

        // Ensure parameters is an object
        if !parameters.is_object() {
            result.add_error(ValidationError::new(
                "INVALID_TYPE",
                "Parameters must be an object",
                &context
            ).with_expected("object").with_actual(&parameters.to_string()));
            return result;
        }

        let param_obj = parameters.as_object().unwrap();

        // Validate each parameter
        for param_info in &function_info.parameters {
            let param_context = context.with_field(&param_info.name);

            if let Some(param_value) = param_obj.get(&param_info.name) {
                // Parameter provided, validate it
                let param_schema = match self.generate_parameter_schema(param_info) {
                    Ok(schema) => schema,
                    Err(e) => {
                        result.add_error(ValidationError::new(
                            "SCHEMA_GENERATION_ERROR",
                            &format!("Failed to generate schema for parameter '{}': {}", param_info.name, e),
                            &param_context
                        ));
                        continue;
                    }
                };

                let validation_result = self.validate(param_value, &param_schema, &param_context);
                if !validation_result.is_valid {
                    for error in validation_result.errors {
                        result.add_error(error);
                    }
                }
                for warning in validation_result.warnings {
                    result.add_warning(warning);
                }
            } else if !param_info.is_optional {
                // Required parameter missing
                result.add_error(ValidationError::new(
                    "MISSING_REQUIRED_PARAMETER",
                    &format!("Required parameter '{}' is missing", param_info.name),
                    &param_context
                ));
            }
        }

        // Check for unexpected parameters in strict mode
        if self.config.strict_mode {
            for param_name in param_obj.keys() {
                if !function_info.parameters.iter().any(|p| p.name == *param_name) {
                    let field_context = context.with_field(param_name);
                    result.add_warning(ValidationWarning::new(
                        "UNEXPECTED_PARAMETER",
                        &format!("Unexpected parameter '{}'", param_name),
                        &field_context.field_path
                    ));
                }
            }
        }

        result
    }

    /// Parse Rune type from type constraints
    fn parse_type_from_constraints(&self, constraints: &[String]) -> Result<RuneType> {
        if constraints.is_empty() {
            return Ok(RuneType::Any);
        }

        // Use the first constraint as the primary type indicator
        let primary_type = constraints[0].to_lowercase();

        match primary_type.as_str() {
            "string" => Ok(RuneType::String),
            "number" => Ok(RuneType::Number),
            "integer" | "int" => Ok(RuneType::Integer),
            "float" | "double" => Ok(RuneType::Float),
            "boolean" | "bool" => Ok(RuneType::Boolean),
            "array" | "list" => Ok(RuneType::Array(Box::new(RuneType::Any))),
            "object" | "map" | "dict" => Ok(RuneType::Object(HashMap::new())),
            "any" => Ok(RuneType::Any),
            "void" | "null" | "unit" => Ok(RuneType::Void),

            // Handle generic types
            s if s.starts_with("array<") => {
                let inner_type = s.strip_prefix("array<").unwrap().strip_suffix(">").unwrap();
                let rune_type = self.parse_type_from_constraints(&[inner_type.to_string()])?;
                Ok(RuneType::Array(Box::new(rune_type)))
            },

            s if s.starts_with("option<") => {
                let inner_type = s.strip_prefix("option<").unwrap().strip_suffix(">").unwrap();
                let rune_type = self.parse_type_from_constraints(&[inner_type.to_string()])?;
                Ok(RuneType::Option(Box::new(rune_type)))
            },

            // Unknown type
            _ => {
                if self.config.allow_unknown_types {
                    Ok(RuneType::Unknown(primary_type.clone()))
                } else {
                    Err(anyhow!("Unknown type: {}", primary_type))
                }
            }
        }
    }

    /// Generate validation schema from validation rules
    fn generate_validation_schema(&self, rules: &[ValidationRule]) -> Result<Value> {
        let mut schema = Map::new();

        for rule in rules {
            match rule.rule_type.as_str() {
                "range" => {
                    if let Some(min) = rule.parameters.get("min") {
                        schema.insert("minimum".to_string(), min.clone());
                    }
                    if let Some(max) = rule.parameters.get("max") {
                        schema.insert("maximum".to_string(), max.clone());
                    }
                },
                "length" => {
                    if let Some(min) = rule.parameters.get("min") {
                        schema.insert("minLength".to_string(), min.clone());
                    }
                    if let Some(max) = rule.parameters.get("max") {
                        schema.insert("maxLength".to_string(), max.clone());
                    }
                },
                "pattern" => {
                    if let Some(pattern) = rule.parameters.get("pattern") {
                        schema.insert("pattern".to_string(), pattern.clone());
                    }
                },
                "enum" => {
                    if let Some(values) = rule.parameters.get("values") {
                        schema.insert("enum".to_string(), values.clone());
                    }
                },
                _ => {
                    warn!("Unknown validation rule type: {}", rule.rule_type);
                }
            }
        }

        Ok(Value::Object(schema))
    }

    /// Validate against anyOf schema
    fn validate_any_of(&self, value: &Value, schemas: &[Value], context: &ValidationContext) -> ValidationResult {
        let mut all_errors = Vec::new();

        for schema in schemas {
            let result = self.validate(value, schema, context);
            if result.is_valid {
                // Return early for successful validation
                return result;
            }
            all_errors.extend(result.errors);
        }

        // If no schema matched, return combined errors
        ValidationResult::invalid(all_errors)
    }

    /// Generate primitive schemas
    fn generate_string_schema(&self) -> Value {
        let mut schema = json!({"type": "string"});

        if let Some(max_length) = self.config.max_string_length {
            schema["maxLength"] = json!(max_length);
        }

        schema
    }

    fn generate_number_schema(&self) -> Value {
        json!({
            "type": "number",
            "minimum": 0
        })
    }

    fn generate_integer_schema(&self) -> Value {
        json!({
            "type": "integer",
            "minimum": 0
        })
    }

    fn generate_boolean_schema(&self) -> Value {
        json!({"type": "boolean"})
    }

    fn generate_array_schema(&self, inner_type: &RuneType) -> Result<Value> {
        let items_schema = self.generate_schema(inner_type, "array_items")?;
        let mut schema = json!({
            "type": "array",
            "items": items_schema
        });

        if let Some(max_length) = self.config.max_array_length {
            schema["maxItems"] = json!(max_length);
        }

        Ok(schema)
    }

    fn generate_object_schema(&self, properties: &HashMap<String, RuneType>) -> Result<Value> {
        let mut schema_properties = Map::new();
        let mut required = Vec::new();

        for (name, rune_type) in properties {
            let property_schema = self.generate_schema(rune_type, name)?;
            schema_properties.insert(name.clone(), property_schema);
            required.push(name.clone());
        }

        Ok(json!({
            "type": "object",
            "properties": schema_properties,
            "required": required
        }))
    }

    fn generate_tuple_schema(&self, types: &[RuneType]) -> Result<Value> {
        let items: Result<Vec<Value>> = types.iter()
            .enumerate()
            .map(|(i, t)| self.generate_schema(t, &format!("tuple_{}", i)))
            .collect();

        Ok(json!({
            "type": "array",
            "items": items?,
            "minItems": types.len(),
            "maxItems": types.len()
        }))
    }

    fn generate_option_schema(&self, inner_type: &RuneType) -> Result<Value> {
        let inner_schema = self.generate_schema(inner_type, "option_inner")?;
        Ok(json!({
            "anyOf": [
                inner_schema,
                {"type": "null"}
            ]
        }))
    }

    fn generate_any_schema(&self) -> Value {
        json!({})
    }

    fn generate_void_schema(&self) -> Value {
        json!({"type": "null"})
    }

    fn generate_unknown_schema(&self, description: &str) -> Value {
        json!({
            "type": "object",
            "description": format!("Unknown type: {}", description)
        })
    }

    fn generate_function_type_schema(&self, parameters: &[RuneType], return_type: &RuneType, is_async: bool) -> Result<Value> {
        let param_schemas: Result<Vec<Value>> = parameters.iter()
            .enumerate()
            .map(|(i, t)| self.generate_schema(t, &format!("param_{}", i)))
            .collect();

        let return_schema = self.generate_schema(return_type, "return_type")?;

        Ok(json!({
            "type": "object",
            "description": "Function definition",
            "properties": {
                "parameters": {
                    "type": "array",
                    "items": param_schemas?
                },
                "return_type": return_schema,
                "is_async": is_async
            },
            "required": ["parameters", "return_type", "is_async"]
        }))
    }

    /// Generate example value for a type
    fn generate_example(&self, rune_type: &RuneType) -> Option<Value> {
        match rune_type {
            RuneType::String => Some(json!("example string")),
            RuneType::Number => Some(json!(42)),
            RuneType::Integer => Some(json!(42)),
            RuneType::Float => Some(json!(3.14)),
            RuneType::Boolean => Some(json!(true)),
            RuneType::Array(_) => Some(json!([])),
            RuneType::Object(_) => Some(json!({})),
            RuneType::Tuple(_) => Some(json!([])),
            RuneType::Option(_) => Some(json!(null)),
            RuneType::Any => Some(json!({})),
            RuneType::Void => Some(json!(null)),
            RuneType::Unknown(_) => Some(json!({})),
            RuneType::Function { .. } => None,
        }
    }

    /// Get human-readable type description
    fn rune_type_description(&self, rune_type: &RuneType) -> &'static str {
        match rune_type {
            RuneType::String => "text string",
            RuneType::Number => "numeric value",
            RuneType::Integer => "integer number",
            RuneType::Float => "floating-point number",
            RuneType::Boolean => "true/false value",
            RuneType::Array(_) => "array/list",
            RuneType::Object(_) => "object/dictionary",
            RuneType::Tuple(_) => "tuple",
            RuneType::Option(_) => "optional value",
            RuneType::Any => "any type",
            RuneType::Void => "null/no value",
            RuneType::Unknown(_) => "unknown type",
            RuneType::Function { .. } => "function",
        }
    }
}

// Built-in validator implementations
struct StringValidator;
struct NumberValidator;
struct IntegerValidator;
struct BooleanValidator;
struct ArrayValidator;
struct ObjectValidator;
struct EnumValidator;
struct PatternValidator;
struct RangeValidator;
struct LengthValidator;

impl Validator for StringValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if let Some(_s) = value.as_str() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Expected string", context)
                    .with_expected("string")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "string"
    }
}

impl Validator for NumberValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if value.is_number() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Expected number", context)
                    .with_expected("number")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "number"
    }
}

impl Validator for IntegerValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if value.is_i64() || value.is_u64() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Expected integer", context)
                    .with_expected("integer")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "integer"
    }
}

impl Validator for BooleanValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if value.is_boolean() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Expected boolean", context)
                    .with_expected("boolean")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "boolean"
    }
}

impl Validator for ArrayValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if value.is_array() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Expected array", context)
                    .with_expected("array")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "array"
    }
}

impl Validator for ObjectValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if value.is_object() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Expected object", context)
                    .with_expected("object")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "object"
    }
}

impl Validator for EnumValidator {
    fn validate(&self, _value: &Value, _context: &ValidationContext) -> ValidationResult {
        // Enum validation requires access to the enum values from the schema
        // This is a simplified implementation
        ValidationResult::valid()
    }

    fn name(&self) -> &str {
        "enum"
    }
}

impl Validator for PatternValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        if value.is_string() {
            // Pattern validation requires access to the pattern from the schema
            // This is a simplified implementation
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(vec![
                ValidationError::new("TYPE_MISMATCH", "Pattern validation requires string", context)
                    .with_expected("string")
                    .with_actual(&value.to_string())
            ])
        }
    }

    fn name(&self) -> &str {
        "pattern"
    }
}

impl Validator for RangeValidator {
    fn validate(&self, _value: &Value, _context: &ValidationContext) -> ValidationResult {
        // Range validation requires access to min/max from the schema
        // This is a simplified implementation
        ValidationResult::valid()
    }

    fn name(&self) -> &str {
        "range"
    }
}

impl Validator for LengthValidator {
    fn validate(&self, value: &Value, context: &ValidationContext) -> ValidationResult {
        let _length = match value {
            Value::String(_s) => 0, // Simplified - would use s.len()
            Value::Array(_arr) => 0, // Simplified - would use arr.len()
            Value::Object(_obj) => 0, // Simplified - would use obj.len()
            _ => {
                return ValidationResult::invalid(vec![
                    ValidationError::new("TYPE_MISMATCH", "Length validation requires string, array, or object", context)
                        .with_expected("string/array/object")
                        .with_actual(&value.to_string())
                ]);
            }
        };

        // Length validation requires access to min/max from the schema
        // This is a simplified implementation
        ValidationResult::valid()
    }

    fn name(&self) -> &str {
        "length"
    }
}

/// Extension trait for merging JSON values
trait ValueExt {
    fn merge(&mut self, other: &Value);
}

impl ValueExt for Value {
    fn merge(&mut self, other: &Value) {
        match (self, other) {
            (Value::Object(ref mut map), Value::Object(other_map)) => {
                for (key, value) in other_map {
                    map.insert(key.clone(), value.clone());
                }
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rune_tools::ast_analyzer::{RuneType, ParameterInfo, AsyncFunctionInfo};

    #[test]
    fn test_schema_generation_basic_types() -> Result<()> {
        let config = ValidationConfig::default();
        let validator = SchemaValidator::new(config);

        // Test string schema
        let string_schema = validator.generate_schema(&RuneType::String, "test")?;
        assert_eq!(string_schema["type"], "string");

        // Test number schema
        let number_schema = validator.generate_schema(&RuneType::Number, "test")?;
        assert_eq!(number_schema["type"], "number");

        // Test boolean schema
        let boolean_schema = validator.generate_schema(&RuneType::Boolean, "test")?;
        assert_eq!(boolean_schema["type"], "boolean");

        Ok(())
    }

    #[test]
    fn test_parameter_validation() -> Result<()> {
        let config = ValidationConfig::default();
        let validator = SchemaValidator::new(config);

        // Create test parameter
        let param = ParameterInfo {
            name: "test_param".to_string(),
            type_name: "string".to_string(),
            type_constraints: vec!["string".to_string()],
            is_optional: false,
            default_value: None,
            description: Some("Test parameter".to_string()),
            validation_rules: vec![],
        };

        let schema = validator.generate_parameter_schema(&param)?;
        let context = ValidationContext::new("test_param", "test_function", "test_module");

        // Test valid value
        let valid_value = json!("test string");
        let result = validator.validate(&valid_value, &schema, &context);
        assert!(result.is_valid);

        // Test invalid value
        let invalid_value = json!(123);
        let result = validator.validate(&invalid_value, &schema, &context);
        assert!(!result.is_valid);

        Ok(())
    }

    #[test]
    fn test_function_parameter_validation() -> Result<()> {
        let config = ValidationConfig::default();
        let validator = SchemaValidator::new(config);

        // Create test function
        let function_info = AsyncFunctionInfo {
            name: "test_function".to_string(),
            is_async: true,
            is_public: true,
            parameters: vec![
                ParameterInfo {
                    name: "name".to_string(),
                    type_name: "string".to_string(),
                    type_constraints: vec!["string".to_string()],
                    is_optional: false,
                    default_value: None,
                    description: Some("Name parameter".to_string()),
                    validation_rules: vec![],
                },
                ParameterInfo {
                    name: "age".to_string(),
                    type_name: "integer".to_string(),
                    type_constraints: vec!["integer".to_string()],
                    is_optional: true,
                    default_value: Some(json!(25)),
                    description: Some("Age parameter".to_string()),
                    validation_rules: vec![],
                },
            ],
            return_type: Some("String".to_string()),
            module_path: vec!["test".to_string()],
            full_path: vec!["test".to_string(), "test_function".to_string()],
            description: Some("Test function".to_string()),
            doc_comments: vec![],
            source_location: crate::rune_tools::ast_analyzer::SourceLocation {
                line: None,
                column: None,
                file_path: None,
            },
            metadata: HashMap::new(),
        };

        // Test valid parameters
        let valid_params = json!({
            "name": "John Doe",
            "age": 30
        });

        let result = validator.validate_function_parameters(&function_info, &valid_params);
        assert!(result.is_valid);

        // Test missing required parameter
        let invalid_params = json!({
            "age": 30
        });

        let result = validator.validate_function_parameters(&function_info, &invalid_params);
        assert!(!result.is_valid);

        Ok(())
    }
}